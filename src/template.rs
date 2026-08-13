use anyhow::{Context, Result};
use regex::Regex;

pub struct TemplateCtx<'a> {
    pub workflow_file: &'a str,
    pub repo: &'a str,
    pub pr_number: &'a str,
    pub version: &'a str,
    pub release_url: Option<&'a str>,
}

const BUILT_IN: &str = r#"This has been automated due to the `<github_workflow_file>` of `<repo>` via PR `<pr_number>`

Bump to `<version>`

{if.projectHasReleaseLogs}
Release Notes: <release_notes_url>
{endif}

i am a robot, beep-boop, so any type of problem must ping the owner, thankies :)"#;

pub fn render(template: &str, ctx: &TemplateCtx) -> Result<String> {
    let mut body = template
        .replace("<github_workflow_file>", ctx.workflow_file)
        .replace("<repo>", ctx.repo)
        .replace("<pr_number>", ctx.pr_number)
        .replace("<version>", ctx.version);

    let block = Regex::new(r"(?s)\{if\.projectHasReleaseLogs\}(.*?)\{endif\}").unwrap();
    body = if ctx.release_url.is_some() {
        block
            .replace_all(&body, |caps: &regex::Captures| {
                caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default()
            })
            .into_owned()
    } else {
        block.replace_all(&body, "").into_owned()
    };

    body = body.replace("\n\n\n", "\n\n");

    if let Some(url) = ctx.release_url {
        body = body.replace("<release_notes_url>", url);
    }
    Ok(body.trim().to_string())
}

pub fn load(root: &std::path::Path, path: Option<&str>) -> Result<String> {
    match path {
        Some(p) => std::fs::read_to_string(root.join(p))
            .with_context(|| format!("cannot read template {p}")),
        None => Ok(BUILT_IN.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_with_release() {
        let ctx = TemplateCtx {
            workflow_file: "publish-vscode.yml",
            repo: "ehshit/EhWebThemesVSCode",
            pr_number: "12",
            version: "1.0.5",
            release_url: Some("https://github.com/ehshit/EhWebThemesVSCode/releases/tag/v1.0.5"),
        };
        let out = render(BUILT_IN, &ctx).unwrap();
        assert!(out.contains("publish-vscode.yml"));
        assert!(out.contains("ehshit/EhWebThemesVSCode"));
        assert!(out.contains("PR `12`"));
        assert!(out.contains("Bump to `1.0.5`"));
        assert!(out.contains("Release Notes: https://github.com/ehshit/EhWebThemesVSCode/releases/tag/v1.0.5"));
        assert!(!out.contains("{if.projectHasReleaseLogs}"));
    }

    #[test]
    fn omits_block_without_release() {
        let ctx = TemplateCtx {
            workflow_file: "publish-zed.yml",
            repo: "ehshit/EhWebThemesZed",
            pr_number: "12",
            version: "1.0.1",
            release_url: None,
        };
        let out = render(BUILT_IN, &ctx).unwrap();
        assert!(!out.contains("Release Notes:"));
        assert!(!out.contains("{if.projectHasReleaseLogs}"));
        assert!(out.contains("i am a robot"));
    }
}