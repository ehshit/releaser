use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

pub struct Git {
    token: String,
}

impl Git {
    pub fn new(token: String) -> Self {
        Git { token }
    }

    pub fn clone(&self, repo: &str, dest: &Path) -> Result<()> {
        let url = format!("https://github.com/{repo}.git");
        run(
            Command::new("git")
                .arg("clone")
                .arg("--quiet")
                .arg(&url)
                .arg(&dest.to_string_lossy().to_string()),
        )?;
        self.auth(dest)
    }

    fn auth(&self, dir: &Path) -> Result<()> {
        run(
            Command::new("git")
                .arg("-C")
                .arg(&dir.to_string_lossy().to_string())
                .args(["config", "http.extraheader"])
                .arg(format!("AUTHORIZATION: Bearer {}", self.token)),
        )
    }

    pub fn checkout(&self, dir: &Path, branch: &str) -> Result<()> {
        run(
            Command::new("git")
                .arg("-C")
                .arg(&dir.to_string_lossy().to_string())
                .args(["checkout", "--quiet", branch]),
        )
    }

    pub fn commit_all(&self, dir: &Path, message: &str) -> Result<()> {
        run(
            Command::new("git")
                .arg("-C")
                .arg(&dir.to_string_lossy().to_string())
                .args(["add", "-A"]),
        )?;
        run(
            Command::new("git")
                .arg("-C")
                .arg(&dir.to_string_lossy().to_string())
                .args(["commit", "--quiet", "-m", message]),
        )
    }

    pub fn push_branch(&self, dir: &Path, branch: &str) -> Result<()> {
        run(
            Command::new("git")
                .arg("-C")
                .arg(&dir.to_string_lossy().to_string())
                .args(["push", "--quiet", "origin", &format!("HEAD:{branch}")]),
        )
    }

    pub fn push_tag(&self, dir: &Path, tag: &str) -> Result<()> {
        run(
            Command::new("git")
                .arg("-C")
                .arg(&dir.to_string_lossy().to_string())
                .args(["tag", tag]),
        )?;
        run(
            Command::new("git")
                .arg("-C")
                .arg(&dir.to_string_lossy().to_string())
                .args(["push", "--quiet", "origin", tag]),
        )
    }

    pub fn has_changes(&self, dir: &Path) -> Result<bool> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&dir.to_string_lossy().to_string())
            .args(["status", "--porcelain"])
            .output()
            .context("failed to run git status")?;
        Ok(!out.stdout.is_empty())
    }
}

fn run(cmd: &mut Command) -> Result<()> {
    let out = cmd.output().context("failed to spawn git")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let what = cmd
            .get_args()
            .next()
            .map(|a| a.to_string_lossy().into_owned())
            .unwrap_or_default();
        anyhow::bail!("git {what} failed: {err}");
    }
    Ok(())
}

pub fn git_show(rev: &str, path: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["show", &format!("{rev}:{path}")])
        .output()
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        None
    }
}

pub fn git_fetch() -> Result<()> {
    run(Command::new("git").args(["fetch", "--quiet", "--prune", "origin"]))
}