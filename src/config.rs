use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Meta {
    #[serde(default = "default_bot_name")]
    pub bot_name: String,
    #[allow(dead_code)]
    #[serde(default = "default_approve_phrase")]
    pub approve_phrase: String,
    #[serde(default)]
    pub categories: Vec<String>,
}

fn default_bot_name() -> String {
    "eh-release-bot".to_string()
}

fn default_approve_phrase() -> String {
    "yes".to_string()
}

#[derive(Debug, Deserialize)]
pub struct Category {
    #[serde(default)]
    pub project: Vec<Project>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub id: String,
    pub repo: String,
    #[serde(default = "default_branch")]
    pub branch: String,
    pub version: String,
    #[serde(default)]
    pub version_files: Vec<VersionFile>,
    #[serde(default)]
    pub changelog_file: Option<String>,
    #[serde(default)]
    pub publish_workflow: Option<String>,
    #[serde(default)]
    pub extension_name: Option<String>,
    #[serde(default)]
    pub github_release: bool,
    #[serde(default)]
    pub release_assets: Vec<String>,
    #[serde(default = "default_pr")]
    pub pr: bool,
    #[serde(default)]
    pub pr_template: Option<String>,
    #[serde(default)]
    pub komac: Option<Komac>,
}

fn default_branch() -> String {
    "main".to_string()
}

fn default_pr() -> bool {
    false
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum VersionFile {
    Simple(String),
    Full {
        path: String,
        #[serde(default = "default_field")]
        field: String,
        #[serde(default)]
        kind: Option<FileKind>,
    },
}

fn default_field() -> String {
    "version".to_string()
}

impl VersionFile {
    pub fn path(&self) -> &str {
        match self {
            VersionFile::Simple(p) => p,
            VersionFile::Full { path, .. } => path,
        }
    }

    pub fn field(&self) -> &str {
        match self {
            VersionFile::Simple(_) => "version",
            VersionFile::Full { field, .. } => field,
        }
    }

    pub fn kind(&self) -> FileKind {
        match self {
            VersionFile::Simple(p) => FileKind::from_path(p),
            VersionFile::Full { kind: Some(k), .. } => *k,
            VersionFile::Full { path, kind: None, .. } => FileKind::from_path(path),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    Json,
    Toml,
    Xml,
    Text,
}

impl FileKind {
    pub fn from_path(path: &str) -> Self {
        match Path::new(path).extension().and_then(|e| e.to_str()) {
            Some("json") => FileKind::Json,
            Some("toml") => FileKind::Toml,
            Some("xml") => FileKind::Xml,
            _ => FileKind::Text,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Komac {
    pub manifest_repo: String,
    pub package: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub url_template: Option<String>,
}

pub fn load_meta(root: &Path) -> Result<Meta> {
    let text = std::fs::read_to_string(root.join("meta.toml"))
        .context("cannot read meta.toml")?;
    let meta: Meta = toml::from_str(&text).context("meta.toml failed to parse")?;
    if meta.categories.is_empty() {
        anyhow::bail!("meta.toml has no categories");
    }
    Ok(meta)
}

pub fn load_categories(root: &Path, meta: &Meta) -> Result<Vec<(String, Category)>> {
    let mut out = Vec::new();
    for cat in &meta.categories {
        let text = std::fs::read_to_string(root.join(cat))
            .with_context(|| format!("cannot read category file {cat}"))?;
        let parsed: Category = toml::from_str(&text)
            .with_context(|| format!("category file {cat} failed to parse"))?;
        out.push((cat.clone(), parsed));
    }
    Ok(out)
}