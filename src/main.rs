mod bump;
mod changelog;
mod config;
mod git;
mod github;
mod template;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    pr: i64,
    #[arg(long)]
    repo: String,
    #[arg(long, default_value = "main")]
    branch: String,
    #[arg(long)]
    dry_run: bool,
    #[arg(long, default_value = ".")]
    root: PathBuf,
}

struct AuthEnv {
    pat: Option<String>,
    app_id: Option<String>,
    installation_id: Option<String>,
    private_key: Option<String>,
}

fn load_auth() -> AuthEnv {
    let private_key = std::env::var("GH_APP_PRIVATE_KEY")
        .ok()
        .or_else(|| {
            std::env::var("GH_APP_PRIVATE_KEY_FILE")
                .ok()
                .and_then(|p| std::fs::read_to_string(p).ok())
        });
    AuthEnv {
        pat: std::env::var("GH_PAT").ok(),
        app_id: std::env::var("GH_APP_ID").ok(),
        installation_id: std::env::var("GH_APP_INSTALLATION_ID").ok(),
        private_key,
    }
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    let auth_env = load_auth();

    let auth = match (auth_env.pat, auth_env.app_id, auth_env.installation_id, auth_env.private_key) {
        (Some(pat), _, _, _) => github::Auth::Pat(pat),
        (None, Some(app_id), Some(installation_id), Some(private_key)) => {
            github::Auth::App { app_id, installation_id, private_key }
        }
        _ => anyhow::bail!("set GH_PAT or GH_APP_ID + GH_APP_INSTALLATION_ID + GH_APP_PRIVATE_KEY"),
    };

    let api = github::Api::new(auth)?;
    let (meta_owner, meta_name) = split_repo(&cli.repo);

    let meta = config::load_meta(&cli.root)?;
    let categories = config::load_categories(&cli.root, &meta)?;

    if !cli.dry_run {
        git::git_fetch().context("cannot fetch meta repo origin")?;
    }
    let pr = api.get_pull(meta_owner, meta_name, cli.pr)?;

    let targets = changed_projects(&cli.root, &categories, &pr.base.sha)?;
    if targets.is_empty() {
        anyhow::bail!("no version bumps detected in PR {} (edit the `version` field in a category toml)", cli.pr);
    }

    log::info!("targeting {} project(s): {:?}", targets.len(), targets.iter().map(|(_, p)| p.id.clone()).collect::<Vec<_>>());

    let blocks = changelog::extract_blocks(pr.body.as_deref().unwrap_or(""));
    log::info!("{} changelog block(s) found", blocks.len());
    for (id, _) in &blocks {
        if !targets.iter().any(|(_, p)| p.id == *id) {
            log::warn!("changelog block <<releases_{id}>> does not match any project id");
        }
    }

    let mut results = Vec::new();
    for (_, project) in &targets {
        log::info!("releasing {} -> {}", project.id, project.version);
        let block = blocks.iter().find(|(id, _)| id == &project.id).map(|(_, b)| b);
        match release_project(&cli, &api, project, block) {
            Ok(r) => results.push(r),
            Err(e) => results.push(ProjectResult::failed(project, e.to_string())),
        }
    }

    if cli.dry_run {
        for r in &results {
            println!("[dry-run] {:<10} version={} published={} pr={} release={}", r.id, r.version, r.published, r.pr_url.as_deref().unwrap_or("-"), r.release_url.as_deref().unwrap_or("-"));
        }
        return Ok(());
    }

    let ok = results.iter().all(|r| r.error.is_none());
    let comment = status_comment(&results, &meta.bot_name);
    api.comment(meta_owner, meta_name, cli.pr, &comment)?;

    if ok {
        api.merge_pull(meta_owner, meta_name, cli.pr)?;
        log::info!("merged meta PR {}", cli.pr);
    } else {
        log::warn!("not merging meta PR {}: some releases failed", cli.pr);
    }
    Ok(())
}

fn changed_projects(_root: &Path, categories: &[(String, config::Category)], base_sha: &str) -> Result<Vec<(String, config::Project)>> {
    let mut out = Vec::new();
    for (cat_path, cat) in categories {
        let base_text = git::git_show(base_sha, cat_path);
        let mut base_versions = std::collections::HashMap::new();
        if let Some(text) = base_text {
            if let Ok(base_cat) = toml::from_str::<config::Category>(&text) {
                for p in base_cat.project {
                    base_versions.insert(p.id.clone(), p.version);
                }
            }
        }
        for project in &cat.project {
            let changed = match base_versions.get(&project.id) {
                Some(old) => old != &project.version,
                None => true,
            };
            if changed {
                out.push((cat_path.clone(), (*project).clone()));
            }
        }
    }
    Ok(out)
}

fn release_project(
    cli: &Cli,
    api: &github::Api,
    project: &config::Project,
    block: Option<&changelog::ReleaseBlock>,
) -> Result<ProjectResult> {
    let workdir = std::env::temp_dir().join(format!("ehrelease-{}-{}", project.id, std::process::id()));
    if workdir.exists() {
        std::fs::remove_dir_all(&workdir).ok();
    }
    std::fs::create_dir_all(&workdir).ok();

    let git = git::Git::new(api.token().to_string());
    let repo_path = workdir.join("repo");
    git.clone(&project.repo, &repo_path)?;
    git.checkout(&repo_path, &project.branch)?;

    for vf in &project.version_files {
        let file_path = repo_path.join(vf.path());
        bump::bump_file(&file_path, vf, &project.version)?;
        log::info!("bumped {} in {}", vf.path(), project.repo);
    }

    if let (Some(rel_file), Some(block)) = (&project.changelog_file, block) {
        let rel_path = repo_path.join(rel_file);
        if changelog::merge_changelog_file(&rel_path, block)? {
            log::info!("updated {} for version {}", rel_file, block.version);
        }
    }

    let dirty = git.has_changes(&repo_path)?;
    if !dirty {
        anyhow::bail!("nothing changed in {}", project.repo);
    }

    let commit_msg = format!("Bump to {}", project.version);
    git.commit_all(&repo_path, &commit_msg)?;
    let tag = format!("v{}", project.version);

    if cli.dry_run {
        std::fs::remove_dir_all(&workdir).ok();
        return Ok(ProjectResult {
            id: project.id.clone(),
            version: project.version.clone(),
            pr_number: None,
            pr_url: None,
            release_url: None,
            published: false,
            error: None,
        });
    }

    let (owner, name) = split_repo(&project.repo);
    let mut pr_number: Option<i64> = None;
    let mut pr_url: Option<String> = None;

    if project.pr {
        let branch = format!("ehrelease/{}/v{}", project.id, project.version);
        git.push_branch(&repo_path, &branch)?;
        git.push_tag(&repo_path, &tag)?;
        let body = render_pr_body(cli, project, None)?;
        let pull = api.create_pull(owner, name, &commit_msg, &branch, &project.branch, &body)?;
        pr_number = Some(pull.number);
        pr_url = Some(pull.html_url);
        log::info!("opened {} PR #{}", project.repo, pull.number);
    } else {
        git.push_branch(&repo_path, &project.branch)?;
        git.push_tag(&repo_path, &tag)?;
        log::info!("pushed {} -> {}", project.repo, project.branch);
    }

    let mut release_url = None;
    let mut published = false;
    if let Some(workflow) = &project.publish_workflow {
        let mut inputs = serde_json::json!({
            "project_id": project.id,
            "repo": project.repo,
            "version": project.version,
            "tag": tag,
            "branch": project.branch,
            "pr_number": cli.pr,
            "meta_repo": cli.repo,
            "github_release": project.github_release,
            "release_assets": project.release_assets.join(","),
        });
        if let Some(name) = &project.extension_name {
            inputs["extension_name"] = serde_json::json!(name);
        }
        if let Some(k) = &project.komac {
            inputs["komac_manifest_repo"] = serde_json::json!(k.manifest_repo);
            inputs["komac_package"] = serde_json::json!(k.package);
            if let Some(v) = &k.version {
                inputs["komac_version"] = serde_json::json!(v);
            }
            if let Some(u) = &k.url {
                inputs["komac_url"] = serde_json::json!(u);
            } else if let Some(u) = &k.url_template {
                inputs["komac_url_template"] = serde_json::json!(u);
            }
        }
        let run_id = api.dispatch_workflow(meta_owner_of(&cli.repo), meta_name_of(&cli.repo), workflow, &cli.branch, inputs)?;
        log::info!("dispatched {workflow} (run {run_id}) for {}", project.repo);

        if project.github_release {
            match wait_for_release(api, owner, name, &tag) {
                Ok(url) => {
                    release_url = Some(url);
                    published = true;
                }
                Err(e) => log::error!("release for {} failed: {e}", project.repo),
            }
        } else {
            published = api.poll_run(meta_owner_of(&cli.repo), meta_name_of(&cli.repo), run_id, Duration::from_secs(600))
                .map(|r| r.conclusion.as_deref() == Some("success"))
                .unwrap_or(false);
        }
    } else {
        published = true;
    }

    if let (Some(num), true) = (pr_number, published) {
        let body = render_pr_body(cli, project, release_url.as_deref())?;
        api.update_pr_body(owner, name, num, &body)?;
        if release_url.is_some() {
            api.merge_pull(owner, name, num)?;
            log::info!("merged {owner}/{name} PR #{num}");
        }
    }

    std::fs::remove_dir_all(&workdir).ok();
    Ok(ProjectResult {
        id: project.id.clone(),
        version: project.version.clone(),
        pr_number,
        pr_url,
        release_url,
        published,
        error: None,
    })
}

fn render_pr_body(cli: &Cli, project: &config::Project, release_url: Option<&str>) -> Result<String> {
    let tpl = template::load(&cli.root, project.pr_template.as_deref())?;
    let ctx = template::TemplateCtx {
        workflow_file: project.publish_workflow.as_deref().unwrap_or("-"),
        repo: &project.repo,
        pr_number: &cli.pr.to_string(),
        version: &project.version,
        release_url,
    };
    template::render(&tpl, &ctx)
}

fn wait_for_release(api: &github::Api, owner: &str, name: &str, tag: &str) -> Result<String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(900);
    let mut delay = Duration::from_secs(2);
    loop {
        if let Some(rel) = api.get_release_by_tag(owner, name, tag)? {
            return Ok(rel.html_url);
        }
        if std::time::Instant::now() > deadline {
            anyhow::bail!("release {tag} never appeared for {owner}/{name}");
        }
        std::thread::sleep(delay);
        delay = (delay * 2).min(Duration::from_secs(60));
    }
}

struct ProjectResult {
    id: String,
    version: String,
    pr_number: Option<i64>,
    pr_url: Option<String>,
    release_url: Option<String>,
    published: bool,
    error: Option<String>,
}

impl ProjectResult {
    fn failed(project: &config::Project, error: String) -> Self {
        ProjectResult {
            id: project.id.clone(),
            version: project.version.clone(),
            pr_number: None,
            pr_url: None,
            release_url: None,
            published: false,
            error: Some(error),
        }
    }
}

fn status_comment(results: &[ProjectResult], bot_name: &str) -> String {
    let mut out = String::from("## ehrelease status\n\n| project | version | published | pr | release |\n| --- | --- | --- | --- | --- |\n");
    for r in results {
        let published = if r.error.is_some() { "error" } else if r.published { "yes" } else { "no" };
        let pr = r.pr_url.as_deref().map(|u| format!("[PR #{}]({u})", r.pr_number.unwrap_or_default())).unwrap_or_else(|| "-".to_string());
        let release = r.release_url.as_deref().map(|u| format!("[release]({u})")).unwrap_or_else(|| "-".to_string());
        out.push_str(&format!("| {} | {} | {} | {} | {} |\n", r.id, r.version, published, pr, release));
        if let Some(err) = &r.error {
            out.push_str(&format!("\n`{}`: {err}\n", r.id));
        }
    }
    out.push_str(&format!("\ni am a robot, beep-boop, so any type of problem must ping the owner of {bot_name}, thankies :)"));
    out
}

fn split_repo(repo: &str) -> (&str, &str) {
    let (owner, name) = repo.split_once('/').unwrap_or((repo, repo));
    (owner, name)
}

fn meta_owner_of(repo: &str) -> &str {
    split_repo(repo).0
}

fn meta_name_of(repo: &str) -> &str {
    split_repo(repo).1
}