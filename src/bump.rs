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
    match find_json_value_range(content, field) {
        Some((start, end)) => {
            let is_string = content.as_bytes().get(start) == Some(&b'"');
            let mut out = String::with_capacity(content.len() + new_version.len() + 2);
            out.push_str(&content[..start]);
            if is_string {
                out.push('"');
                out.push_str(new_version);
                out.push('"');
            } else {
                out.push_str(new_version);
            }
            out.push_str(&content[end..]);
            Ok(out)
        }
        None => anyhow::bail!("no json field `{field}` found"),
    }
}

fn find_json_value_range(content: &str, field: &str) -> Option<(usize, usize)> {
    let parts: Vec<&str> = field.split('.').collect();
    let bytes = content.as_bytes();
    let start = skip_ws(bytes, 0);
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    find_json_value_in_object(bytes, start, &parts, 0)
}

fn find_json_value_in_object(bytes: &[u8], mut pos: usize, parts: &[&str], depth: usize) -> Option<(usize, usize)> {
    let want = parts.get(depth)?;
    pos += 1;
    loop {
        pos = skip_ws(bytes, pos);
        match bytes.get(pos)? {
            b'}' => return None,
            b',' => pos += 1,
            b'"' => {
                let (key_start, key_end) = read_json_string(bytes, pos)?;
                let key = std::str::from_utf8(&bytes[key_start + 1..key_end - 1]).ok()?;
                pos = skip_ws(bytes, key_end);
                if bytes.get(pos) != Some(&b':') {
                    return None;
                }
                pos = skip_ws(bytes, pos + 1);
                if key == *want {
                    if depth == parts.len() - 1 {
                        return json_value_span(bytes, pos);
                    }
                    if bytes.get(pos) == Some(&b'{') {
                        return find_json_value_in_object(bytes, pos, parts, depth + 1);
                    }
                    return None;
                }
                pos = skip_json_value(bytes, pos)?;
            }
            _ => return None,
        }
    }
}

fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'\n' || bytes[i] == b'\r') {
        i += 1;
    }
    i
}

fn read_json_string(bytes: &[u8], mut i: usize) -> Option<(usize, usize)> {
    let start = i;
    i += 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some((start, i + 1)),
            _ => i += 1,
        }
    }
    None
}

fn skip_json_value(bytes: &[u8], mut i: usize) -> Option<usize> {
    match bytes.get(i)? {
        b'"' => {
            let (_, end) = read_json_string(bytes, i)?;
            Some(end)
        }
        b'{' => {
            let mut depth = 1;
            i += 1;
            while i < bytes.len() && depth > 0 {
                match bytes[i] {
                    b'"' => {
                        let (_, end) = read_json_string(bytes, i)?;
                        i = end;
                    }
                    b'{' => depth += 1,
                    b'}' => depth -= 1,
                    _ => i += 1,
                }
            }
            if depth == 0 { Some(i) } else { None }
        }
        b'[' => {
            let mut depth = 1;
            i += 1;
            while i < bytes.len() && depth > 0 {
                match bytes[i] {
                    b'"' => {
                        let (_, end) = read_json_string(bytes, i)?;
                        i = end;
                    }
                    b'[' => depth += 1,
                    b']' => depth -= 1,
                    _ => i += 1,
                }
            }
            if depth == 0 { Some(i) } else { None }
        }
        _ => {
            while i < bytes.len() && !matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r' | b',' | b'}' | b']') {
                i += 1;
            }
            Some(i)
        }
    }
}

fn json_value_span(bytes: &[u8], pos: usize) -> Option<(usize, usize)> {
    match bytes.get(pos)? {
        b'"' => read_json_string(bytes, pos),
        _ => {
            let end = skip_json_value(bytes, pos)?;
            Some((pos, end))
        }
    }
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
    let pat_attr = format!(r#"(<{}\b[^>]*?\bvalue\s*=\s*)"[^"]*""#, regex::escape(field));
    let re_attr = Regex::new(&pat_attr)?;
    if re_attr.is_match(content) {
        return Ok(re_attr
            .replace(content, |caps: &regex::Captures| {
                format!(r#"{}"{new_version}""#, &caps[1])
            })
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
    fn json_dotted_keeps_formatting() {
        let input = "{\n  \"publish\": {\n    \"version\": \"1.0.4\",\n    \"other\": \"kept\"\n  },\n  \"top\": \"untouched\"\n}\n";
        let out = bump_content(input, "publish.version", "1.0.5", FileKind::Json).unwrap();
        assert_eq!(
            out,
            "{\n  \"publish\": {\n    \"version\": \"1.0.5\",\n    \"other\": \"kept\"\n  },\n  \"top\": \"untouched\"\n}\n"
        );
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
    fn xml_attr_value_not_first() {
        let out = bump_content("<plugin id=\"x\" value=\"1.0\" />", "plugin", "1.1", FileKind::Xml).unwrap();
        assert_eq!(out, "<plugin id=\"x\" value=\"1.1\" />");
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