use anyhow::{Context, Result};
use base64::Engine;
use reqwest::blocking::{Client, Response};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub enum Auth {
    Pat(String),
    App {
        app_id: String,
        installation_id: String,
        private_key: String,
    },
}

pub struct Api {
    client: Client,
    base: String,
    token: String,
}

#[derive(Deserialize)]
pub struct Pull {
    pub number: i64,
    #[allow(dead_code)]
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    pub html_url: String,
    pub base: BranchRef,
    #[allow(dead_code)]
    pub head: BranchRef,
}

#[derive(Deserialize)]
pub struct BranchRef {
    #[serde(rename = "ref")]
    #[allow(dead_code)]
    pub branch: String,
    pub sha: String,
}

#[derive(Deserialize)]
pub struct Release {
    #[allow(dead_code)]
    pub tag_name: String,
    pub html_url: String,
}

#[derive(Serialize)]
struct DispatchBody<'a> {
    #[serde(rename = "ref")]
    pub branch: &'a str,
    pub inputs: serde_json::Value,
}

#[derive(Serialize)]
struct PullBody<'a> {
    pub title: &'a str,
    pub head: &'a str,
    pub base: &'a str,
    pub body: &'a str,
}

#[derive(Serialize)]
struct MergeBody {
    pub merge_method: &'static str,
}

#[derive(Serialize)]
struct CommentBody<'a> {
    pub body: &'a str,
}

#[derive(Serialize)]
struct BodyOnly<'a> {
    pub body: &'a str,
}

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
}

#[derive(Deserialize)]
struct RunsResponse {
    workflow_runs: Vec<WorkflowRun>,
}

#[derive(Deserialize)]
pub struct WorkflowRun {
    pub id: i64,
    pub status: String,
    #[serde(default)]
    pub conclusion: Option<String>,
}

impl Api {
    pub fn new(auth: Auth) -> Result<Api> {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .context("failed to build http client")?;
        let token = match auth {
            Auth::Pat(pat) => pat,
            Auth::App { app_id, installation_id, private_key } => {
                mint_installation_token(&client, &app_id, &installation_id, &private_key)?
            }
        };
        Ok(Api {
            client,
            base: "https://api.github.com".to_string(),
            token,
        })
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    fn send(&self, req: reqwest::blocking::RequestBuilder) -> Result<Response> {
        let resp = req
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "ehrelease")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .context("github request failed")?;
        Ok(resp)
    }

    fn get_opt(&self, url: &str) -> Result<Option<Response>> {
        let resp = self.send(self.client.get(format!("{}{}", self.base, url)))?;
        match resp.status() {
            StatusCode::NOT_FOUND => Ok(None),
            s if s.is_success() => Ok(Some(resp)),
            s => {
                let body = resp.text().unwrap_or_default();
                anyhow::bail!("GET {url} -> {s}: {body}")
            }
        }
    }

    fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let resp = self
            .get_opt(url)?
            .with_context(|| format!("404 on GET {url}"))?;
        resp.json().context("bad json from github")
    }

    fn post_json<T: DeserializeOwned>(&self, url: &str, body: &impl Serialize) -> Result<T> {
        let resp = self.send(self.client.post(format!("{}{}", self.base, url)).json(body))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            anyhow::bail!("POST {url} -> {status}: {text}");
        }
        resp.json().context("bad json from github")
    }

    fn patch(&self, url: &str, body: &impl Serialize) -> Result<()> {
        let resp = self.send(self.client.patch(format!("{}{}", self.base, url)).json(body))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            anyhow::bail!("PATCH {url} -> {status}: {text}");
        }
        Ok(())
    }

    pub fn get_pull(&self, owner: &str, repo: &str, number: i64) -> Result<Pull> {
        self.get_json(&format!("/repos/{owner}/{repo}/pulls/{number}"))
    }

    pub fn create_pull(&self, owner: &str, repo: &str, title: &str, head: &str, base: &str, body: &str) -> Result<Pull> {
        let payload = PullBody { title, head, base, body };
        self.post_json(&format!("/repos/{owner}/{repo}/pulls"), &payload)
    }

    pub fn merge_pull(&self, owner: &str, repo: &str, number: i64) -> Result<()> {
        let payload = MergeBody { merge_method: "merge" };
        let resp = self.send(
            self.client
                .put(format!("{}/repos/{owner}/{repo}/pulls/{number}/merge", self.base))
                .json(&payload),
        )?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            anyhow::bail!("merge PR {number} -> {status}: {text}");
        }
        Ok(())
    }

    pub fn update_pr_body(&self, owner: &str, repo: &str, number: i64, body: &str) -> Result<()> {
        let payload = BodyOnly { body };
        self.patch(&format!("/repos/{owner}/{repo}/pulls/{number}"), &payload)
    }

    pub fn comment(&self, owner: &str, repo: &str, number: i64, body: &str) -> Result<()> {
        let payload = CommentBody { body };
        self.post_json::<serde_json::Value>(
            &format!("/repos/{owner}/{repo}/issues/{number}/comments"),
            &payload,
        )
        .map(|_| ())
    }

    pub fn dispatch_workflow(&self, owner: &str, repo: &str, workflow: &str, branch: &str, inputs: serde_json::Value) -> Result<i64> {
        let payload = DispatchBody { branch, inputs };
        self.post_json::<serde_json::Value>(
            &format!("/repos/{owner}/{repo}/actions/workflows/{workflow}/dispatches"),
            &payload,
        )?;
        std::thread::sleep(Duration::from_secs(3));
        let runs: RunsResponse = self.get_json(&format!(
            "/repos/{owner}/{repo}/actions/workflows/{workflow}/runs?event=workflow_dispatch&per_page=1"
        ))?;
        runs.workflow_runs
            .first()
            .map(|r| r.id)
            .context("no workflow run found after dispatch")
    }

    pub fn poll_run(&self, owner: &str, repo: &str, run_id: i64, timeout: Duration) -> Result<WorkflowRun> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let run: WorkflowRun =
                self.get_json(&format!("/repos/{owner}/{repo}/actions/runs/{run_id}"))?;
            if run.status == "completed" {
                return Ok(run);
            }
            if std::time::Instant::now() > deadline {
                anyhow::bail!("workflow run {run_id} did not finish in time");
            }
            std::thread::sleep(Duration::from_secs(10));
        }
    }

    pub fn get_release_by_tag(&self, owner: &str, repo: &str, tag: &str) -> Result<Option<Release>> {
        match self.get_opt(&format!("/repos/{owner}/{repo}/releases/tags/{tag}"))? {
            None => Ok(None),
            Some(resp) => Ok(Some(resp.json()?)),
        }
    }
}

fn normalize_private_key(raw: &str) -> Result<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        anyhow::bail!("GH_APP_PRIVATE_KEY is empty");
    }
    if raw.contains("-----BEGIN") {
        let unescaped = raw.replace("\\n", "\n");
        if !unescaped.contains("-----BEGIN") {
            anyhow::bail!("private key has no PEM header");
        }
        return Ok(unescaped);
    }
    let compact = raw.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(compact)
        .context("GH_APP_PRIVATE_KEY is neither raw PEM nor base64")?;
    let decoded = String::from_utf8(decoded).context("decoded private key is not utf-8")?;
    if !decoded.contains("-----BEGIN") {
        anyhow::bail!("decoded private key has no PEM header");
    }
    normalize_private_key(&decoded)
}

fn mint_installation_token(client: &Client, app_id: &str, installation_id: &str, pem: &str) -> Result<String> {
    let pem = normalize_private_key(pem)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let claims = jwt_claims(now, app_id);
    let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    let key = jsonwebtoken::EncodingKey::from_rsa_pem(pem.as_bytes())
        .context("invalid app private key pem")?;
    let jwt = jsonwebtoken::encode(&header, &claims, &key).context("failed to sign app jwt")?;

    let resp = client
        .post(format!(
            "https://api.github.com/app/installations/{installation_id}/access_tokens"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "ehrelease")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .context("failed to request installation token")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        anyhow::bail!("installation token -> {status}: {text}");
    }
    let parsed: TokenResponse = resp.json()?;
    Ok(parsed.token)
}

fn jwt_claims(now: u64, app_id: &str) -> serde_json::Value {
    serde_json::json!({
        "iat": now - 60,
        "exp": now + 540,
        "iss": app_id,
    })
}