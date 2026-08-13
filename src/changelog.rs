use anyhow::{Context, Result};
use regex::Regex;

pub struct ReleaseBlock {
    pub version: String,
    pub section: String,
}

pub fn extract_blocks(pr_body: &str) -> Vec<(String, ReleaseBlock)> {
    let mut out = Vec::new();
    let marker = Regex::new(r"(?m)^<<releases[-_]([A-Za-z0-9_-]+)>>\s*$").unwrap();
    let fence = Regex::new(r"(?ms)^```[^\n]*\n(.*?)^```").unwrap();

    let mut pos = 0;
    while let Some(m) = marker.find_at(pr_body, pos) {
        let id = marker.captures(&pr_body[m.start()..]).unwrap()[1].to_string();
        let rest = &pr_body[m.end()..];
        if let Some(f) = fence.captures(rest) {
            let fstart = fence.find(rest).unwrap().start();
            let inner = f.get(1).unwrap().as_str();
            let abs = m.end() + fstart;
            if let Some(block) = parse_section(inner) {
                out.push((id, block));
            }
            pos = abs + f.get(0).unwrap().len();
        } else {
            break;
        }
    }
    out
}

fn parse_section(text: &str) -> Option<ReleaseBlock> {
    let heading = Regex::new(r"(?m)^(#{1,6})\s*\[([^\]]+)\]").unwrap();
    let caps = heading.captures(text)?;
    let version = caps[2].trim().to_string();
    let section = text.trim().to_string();
    Some(ReleaseBlock { version, section })
}

pub fn merge_changelog(existing: &str, version: &str, section: &str) -> String {
    let sec_re = Regex::new(&format!(r"(?m)^(#{{1,6}})\s*\[{}\][ \t]*\r?$", regex::escape(version)))
        .unwrap();

    if let Some(m) = sec_re.find(existing) {
        let level = sec_re.captures(existing).unwrap()[1].len();
        let start = m.start();
        let end = next_section_end(existing, m.end(), level);
        let mut out = String::with_capacity(existing.len());
        out.push_str(&existing[..start]);
        out.push_str(section.trim_end());
        out.push('\n');
        out.push_str(&existing[end..]);
        return out;
    }

    let lines: Vec<&str> = existing.lines().collect();
    let title_end = lines.iter().position(|l| l.starts_with('#')).map(|i| i + 1);
    let insert_at = match title_end {
        Some(i) if i < lines.len() => i,
        _ => 0,
    };

    let mut out = Vec::new();
    let lc = lines.clone();
    if insert_at > 0 {
        for line in &lc[..insert_at] {
            out.push(*line);
        }
        out.push("");
    }
    out.push("");
    for line in section.trim_end().lines() {
        out.push(line);
    }
    out.push("");
    for line in &lc[insert_at..] {
        out.push(*line);
    }
    out.join("\n") + "\n"
}

fn next_section_end(content: &str, from: usize, level: usize) -> usize {
    let re = Regex::new(r"(?m)^(#{1,6})\s").unwrap();
    for m in re.find_iter(&content[from..]) {
        let mlevel = re.captures(&content[from..]).unwrap()[1].len();
        if mlevel <= level {
            return from + m.start();
        }
    }
    content.len()
}

pub fn merge_changelog_file(
    path: &std::path::Path,
    block: &ReleaseBlock,
) -> Result<bool> {
    let existing = if path.exists() {
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?
    } else {
        String::new()
    };
    let merged = merge_changelog(&existing, &block.version, &block.section);
    if merged == existing {
        return Ok(false);
    }
    std::fs::write(path, &merged).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> String {
        format!(
            "releasing stuff\n\n<<releases-vscode>>\n```\n# [1.0.5]\n\n- added the color pink\n```\n\n<<releases-zed>>\n```\n# [1.0.1]\n\n- zed things\n```\n"
        )
    }

    #[test]
    fn extracts_two_blocks() {
        let blocks = extract_blocks(&body());
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].0, "vscode");
        assert_eq!(blocks[0].1.version, "1.0.5");
        assert!(blocks[0].1.section.contains("color pink"));
        assert_eq!(blocks[1].0, "zed");
    }

    #[test]
    fn extracts_underscore_marker() {
        let input = "stuff\n\n<<releases_vscode>>\n```\n# [1.0.5]\n\n- pink\n```\n";
        let blocks = extract_blocks(input);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, "vscode");
    }

    #[test]
    fn prepends_new_version() {
        let existing = "# Change Log\n\n## [1.0.4]\n\n- old stuff\n";
        let section = "# [1.0.5]\n\n- new stuff";
        let merged = merge_changelog(existing, "1.0.5", section);
        assert!(merged.contains("[1.0.5]"));
        assert!(merged.contains("- new stuff"));
        assert!(merged.contains("[1.0.4]"));
        assert!(merged.contains("- old stuff"));
        let pos5 = merged.find("1.0.5").unwrap();
        let pos4 = merged.find("1.0.4").unwrap();
        assert!(pos5 < pos4);
    }

    #[test]
    fn replaces_existing_version() {
        let existing = "# Change Log\n\n## [1.0.5]\n\n- stale\n\n## [1.0.4]\n\n- old\n";
        let section = "# [1.0.5]\n\n- fresh";
        let merged = merge_changelog(existing, "1.0.5", section);
        assert!(merged.contains("- fresh"));
        assert!(!merged.contains("- stale"));
        assert!(merged.contains("[1.0.4]"));
    }
}