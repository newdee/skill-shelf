//! SKILL.md parsing and Agent Skills spec validation.
//!
//! Spec: <https://agentskills.io/specification>. These are pure helpers; the
//! decision of *when* to validate (import, commit) lives in the server layer.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The frontmatter of a SKILL.md file (Agent Skills spec).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    #[serde(
        default,
        rename = "allowed-tools",
        skip_serializing_if = "Option::is_none"
    )]
    pub allowed_tools: Option<String>,
}

/// Parse the YAML frontmatter of a SKILL.md file into [`SkillMeta`].
pub fn parse(bytes: &[u8]) -> Result<SkillMeta> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Error::Invalid("SKILL.md is not valid UTF-8".into()))?;
    let front = extract_frontmatter(text)
        .ok_or_else(|| Error::Invalid("SKILL.md is missing a YAML frontmatter block (---)".into()))?;
    serde_yaml_ng::from_str(front)
        .map_err(|e| Error::Invalid(format!("invalid SKILL.md frontmatter: {e}")))
}

/// Validate frontmatter against the Agent Skills spec.
pub fn validate(meta: &SkillMeta) -> Result<()> {
    validate_name(&meta.name)?;
    if meta.description.trim().is_empty() {
        return Err(Error::Invalid("description must not be empty".into()));
    }
    if meta.description.chars().count() > 1024 {
        return Err(Error::Invalid("description exceeds 1024 characters".into()));
    }
    if let Some(c) = &meta.compatibility {
        if c.chars().count() > 500 {
            return Err(Error::Invalid("compatibility exceeds 500 characters".into()));
        }
    }
    Ok(())
}

/// Validate a skill `name` against the spec rules.
pub fn validate_name(name: &str) -> Result<()> {
    let len = name.chars().count();
    if len == 0 || len > 64 {
        return Err(Error::Invalid("name must be 1-64 characters".into()));
    }
    if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(Error::Invalid(
            "name may only contain lowercase letters, digits, and hyphens".into(),
        ));
    }
    if name.starts_with('-') || name.ends_with('-') {
        return Err(Error::Invalid("name must not start or end with a hyphen".into()));
    }
    if name.contains("--") {
        return Err(Error::Invalid("name must not contain consecutive hyphens".into()));
    }
    Ok(())
}

/// Relative file paths referenced by a SKILL.md body: markdown link targets
/// `](path)` and any `scripts/` / `references/` / `assets/` path tokens.
pub fn referenced_paths(skill_md: &str) -> Vec<String> {
    let mut out = Vec::new();

    // Markdown links / images: `](target)`.
    let mut i = 0;
    while let Some(pos) = skill_md[i..].find("](") {
        let start = i + pos + 2;
        match skill_md[start..].find(')') {
            Some(end) => {
                out.push(skill_md[start..start + end].trim().to_string());
                i = start + end + 1;
            }
            None => break,
        }
    }

    // Bare dir-prefixed paths mentioned in prose or code.
    let delim = |c: char| {
        c.is_whitespace() || matches!(c, ')' | '(' | '"' | '\'' | '`' | '<' | '>' | '[' | ']' | ',' | ';' | ':')
    };
    for prefix in ["scripts/", "references/", "assets/"] {
        let mut j = 0;
        while let Some(pos) = skill_md[j..].find(prefix) {
            let start = j + pos;
            let rest = &skill_md[start..];
            let end = rest.find(delim).unwrap_or(rest.len());
            out.push(rest[..end].to_string());
            j = start + end.max(1);
        }
    }
    out
}

/// Referenced relative files that are NOT present in `existing` paths — a lint
/// warning (not a spec violation). External links and anchors are ignored.
pub fn missing_references(skill_md: &str, existing: &[String]) -> Vec<String> {
    let set: std::collections::HashSet<&str> = existing.iter().map(String::as_str).collect();
    let mut missing: Vec<String> = referenced_paths(skill_md)
        .into_iter()
        // strip anchor, keep path part
        .map(|t| t.split('#').next().unwrap_or("").trim().to_string())
        .filter(|t| {
            !t.is_empty()
                && !t.starts_with("http://")
                && !t.starts_with("https://")
                && !t.starts_with('/')
                && !t.starts_with("mailto:")
                && !set.contains(t.as_str())
        })
        .collect();
    missing.sort();
    missing.dedup();
    missing
}

/// Return the YAML between the leading `---` and the next `---` line, if present.
fn extract_frontmatter(text: &str) -> Option<&str> {
    let t = text.trim_start_matches('\u{feff}').trim_start();
    let rest = t.strip_prefix("---")?;
    let rest = rest.strip_prefix('\r').unwrap_or(rest);
    let rest = rest.strip_prefix('\n')?;
    let end = rest.find("\n---")?;
    Some(&rest[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_frontmatter() {
        let md = b"---\nname: pdf-parse\ndescription: Parse PDF files\nlicense: Apache-2.0\nmetadata:\n  author: acme\n  version: \"1.0\"\n---\n# Body";
        let m = parse(md).unwrap();
        assert_eq!(m.name, "pdf-parse");
        assert_eq!(m.description, "Parse PDF files");
        assert_eq!(m.license.as_deref(), Some("Apache-2.0"));
        assert_eq!(m.metadata.get("author").map(String::as_str), Some("acme"));
        validate(&m).unwrap();
    }

    #[test]
    fn missing_frontmatter_errors() {
        assert!(parse(b"# just markdown").is_err());
    }

    #[test]
    fn name_rules() {
        assert!(validate_name("pdf-parse").is_ok());
        assert!(validate_name("a1").is_ok());
        assert!(validate_name("PDF").is_err()); // uppercase
        assert!(validate_name("-pdf").is_err()); // leading hyphen
        assert!(validate_name("pdf-").is_err()); // trailing hyphen
        assert!(validate_name("pdf--parse").is_err()); // consecutive
        assert!(validate_name("").is_err());
    }

    #[test]
    fn lint_finds_missing_references() {
        let md = "# Skill\nRun scripts/run.py and see [guide](references/guide.md).\nExternal: [x](https://ok.com).";
        let existing = vec!["SKILL.md".to_string(), "scripts/run.py".to_string()];
        let missing = missing_references(md, &existing);
        assert_eq!(missing, vec!["references/guide.md".to_string()]);
        // external link and existing script are not flagged
        assert!(!missing.iter().any(|m| m.contains("http")));
        assert!(!missing.iter().any(|m| m == "scripts/run.py"));
    }

    #[test]
    fn empty_description_rejected() {
        let m = SkillMeta {
            name: "ok".into(),
            description: "  ".into(),
            ..Default::default()
        };
        assert!(validate(&m).is_err());
    }
}
