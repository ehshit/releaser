use crate::config::{FileKind, VersionFile};
use anyhow::{Context, Result};
use regex::Regex;

pub fn bump_content(content: &str, field: &str, new_version: &str, kind: FileKind) -> Result<String> {
    match kind {
        FileKind::Json => bump_json(content, field, new_version),
        FileKind::Toml => bump_toml(content, field, new_version),
        FileKind::Xml => bump_xml(content, field, new_version),
        FileKind::Text => bump_text(content, field, new_version),
    }
}

pub fn bump_file(path: &std::path::Path, vf: &VersionFile, new_version: &str) -> Result<String> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let bumped =
        bump_content(&content, vf.field(), new_version, vf.kind())
            .with_context(|| format!("bumping {} failed", path.display()))?;
    std::fs::write(path, &bumped)
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(bumped)
}

fn bump_json(content: &str, field: &str, new_version: &str) -> Result<String> {
    if !field.contains('.') {
        let pat = format!(r#""{}"\s*:\s*"[^"]*""#, regex::escape(field));
        let re = Regex::new(&pat)?;
        if re.is_match(content) {
            return Ok(re.replace(content, |_: &regex::Captures| {
                format!(r#""{field}": "{new_version}""#)
            })
            .into_owned());
        }
    }
    let mut value: serde_json::Value =
        serde_json::from_str(content).context("json parse fallback failed")?;
    let mut cur = &mut value;
    for part in field.split('.') {
        cur = cur
            .get_mut(part)
            .with_context(|| format!("no json field `{part}` in path `{field}`"))?;
    }
    *cur = serde_json::Value::String(new_version.to_string());
    serde_json::to_string_pretty(&value).context("json serialize fallback failed")
}

fn bump_toml(content: &str, field: &str, new_version: &str) -> Result<String> {
    let pat = format!(r#"(?m)^[ \t]*{}[ \t]*=[ \t]*"[^"]*""#, regex::escape(field));
    let re = Regex::new(&pat)?;
    if re.is_match(content) {
        return Ok(re.replace(content, format!("{field} = \"{new_version}\"")).into_owned());
    }
    anyhow::bail!("no toml field `{field}` found at top level")
}

fn bump_xml(content: &str, field: &str, new_version: &str) -> Result<String> {
    let pat = format!(r"<{}\s*>[^<]*</{}>", regex::escape(field), regex::escape(field));
    let re = Regex::new(&pat)?;
    if re.is_match(content) {
        return Ok(re.replace(content, format!("<{field}>{new_version}</{field}>")).into_owned());
    }
    let pat_attr = format!(r#"<{}\s+value\s*=\s*"[^"]*""#, regex::escape(field));
    let re_attr = Regex::new(&pat_attr)?;
    if re_attr.is_match(content) {
        return Ok(re_attr
            .replace(content, format!("<{field} value=\"{new_version}\""))
            .into_owned());
    }
    anyhow::bail!("no xml element `<{field}>` found")
}

fn bump_text(content: &str, field: &str, new_version: &str) -> Result<String> {
    let re = Regex::new(field).context("invalid regex in version_field for text file")?;
    if re.captures(content).is_none() {
        anyhow::bail!("regex `{field}` found no match in text file");
    }
    Ok(re
        .replace_all(content, |caps: &regex::Captures| {
            let m = caps.get(0).unwrap();
            let g1 = caps.get(1).unwrap();
            let mut out = String::new();
            out.push_str(&content[m.start()..g1.start()]);
            out.push_str(new_version);
            out.push_str(&content[g1.end()..m.end()]);
            out
        })
        .into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_top_level() {
        let out = bump_content(r#"{ "version": "1.0.4" }"#, "version", "1.0.5", FileKind::Json).unwrap();
        assert!(out.contains("\"version\": \"1.0.5\""));
    }

    #[test]
    fn toml_top_level() {
        let out = bump_content("version = \"1.0\"\n", "version", "1.0.1", FileKind::Toml).unwrap();
        assert!(out.contains("version = \"1.0.1\""));
    }

    #[test]
    fn xml_element() {
        let out = bump_content("<version>1.0</version>", "version", "1.1", FileKind::Xml).unwrap();
        assert!(out.contains("<version>1.1</version>"));
    }

    #[test]
    fn text_regex() {
        let out = bump_content(
            "version = \"1.0.4\"",
            r#"version\s*=\s*"([0-9.]+)""#,
            "1.0.5",
            FileKind::Text,
        )
        .unwrap();
        assert!(out.contains("version = \"1.0.5\""));
    }
}