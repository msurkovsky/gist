//! Skill metadata and namespaced references; see docs/skill-contract.md and
//! docs/cases/packaging.md. Parsing never executes YAML includes or commands.

use regex::{Captures, Regex};
use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) struct Metadata<'a> {
    pub fields: Value,
    pub header: &'a str,
}

/// Build a package namespace, refusing names that flatten to the same target.
pub(crate) fn namespace(
    package: &str,
    names: impl IntoIterator<Item = String>,
) -> Result<BTreeMap<String, String>, String> {
    let mut result = BTreeMap::new();
    for name in names {
        if result
            .insert(name.clone(), format!("{package}-{name}"))
            .is_some()
        {
            return Err(format!(
                "experimental/{package}: duplicate skill name {name}"
            ));
        }
    }
    Ok(result)
}

pub(crate) fn parse(text: &str) -> Result<Metadata<'_>, String> {
    let mut lines = text.split_inclusive('\n');
    if lines.next().map(str::trim_end) != Some("---") {
        return Err("missing opening frontmatter delimiter".into());
    }
    let start = text.find('\n').ok_or("unclosed frontmatter")? + 1;
    let mut end = start;
    for line in lines {
        if line.trim_end() == "---" {
            let header = &text[start..end];
            let fields: Value = serde_saphyr::from_str(header)
                .map_err(|err| format!("invalid YAML frontmatter: {err}"))?;
            if !fields.is_object() {
                return Err("frontmatter must be a mapping".into());
            }
            for key in ["name", "description"] {
                if !fields[key].as_str().is_some_and(|s| !s.trim().is_empty()) {
                    return Err(format!("{key} must be a nonempty string"));
                }
            }
            if fields
                .get("disable-model-invocation")
                .is_some_and(|v| !v.is_boolean())
            {
                return Err("disable-model-invocation must be a boolean".into());
            }
            return Ok(Metadata { fields, header });
        }
        end += line.len();
    }
    Err("unclosed frontmatter".into())
}

/// Rewrite only supported invocation syntax, using the complete package name map.
pub(crate) fn rewrite_references(text: &str, names: &BTreeMap<String, String>) -> String {
    let invocation = Regex::new(r"[/\$]([a-z][a-z0-9-]*)").expect("invocation regex");
    let quoted = Regex::new(r#"(["`])([a-z][a-z0-9-]*)(["`])"#).expect("quoted name regex");
    let rewrite = |paragraph: &str| {
        let rewritten = invocation.replace_all(paragraph, |caps: &Captures<'_>| {
            let whole = caps.get(0).unwrap();
            let before = paragraph[..whole.start()].chars().next_back();
            let after = &paragraph[whole.end()..];
            let boundary = before.is_none_or(|c| !c.is_alphanumeric() && !"_./:-".contains(c));
            let resource = after.starts_with('/')
                || after.starts_with('_')
                || after
                    .strip_prefix('.')
                    .is_some_and(|s| s.starts_with(char::is_alphanumeric));
            match names.get(&caps[1]) {
                Some(name) if boundary && !resource => format!("{}{name}", &caps[0][..1]),
                _ => caps[0].to_string(),
            }
        });
        if !paragraph.contains("Skill tool") {
            return rewritten.into_owned();
        }
        quoted
            .replace_all(&rewritten, |caps: &Captures<'_>| {
                match names.get(&caps[2]) {
                    Some(name) if caps[1] == caps[3] => {
                        format!("{}{name}{}", &caps[1], &caps[3])
                    }
                    _ => caps[0].to_string(),
                }
            })
            .into_owned()
    };
    let boundaries = Regex::new(r"\r?\n[ \t]*\r?\n").expect("paragraph regex");
    let mut output = String::with_capacity(text.len());
    let mut start = 0;
    for boundary in boundaries.find_iter(text) {
        output.push_str(&rewrite(&text[start..boundary.start()]));
        output.push_str(boundary.as_str());
        start = boundary.end();
    }
    output.push_str(&rewrite(&text[start..]));
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    // Case: docs/cases/packaging.md#package-line-endings
    #[test]
    fn review_fix_crlf_paragraphs_keep_quoted_prose_outside_the_invocation() {
        let names = BTreeMap::from([("tdd".into(), "vendor-tdd".into())]);
        for newline in ["\n", "\r\n"] {
            for blank in ["", " \t"] {
                let source = format!("Call the Skill tool with \"tdd\".{newline}{blank}{newline}Keep \"tdd\" as ordinary prose.{newline}");
                let expected = format!("Call the Skill tool with \"vendor-tdd\".{newline}{blank}{newline}Keep \"tdd\" as ordinary prose.{newline}");
                assert_eq!(rewrite_references(&source, &names), expected);
            }
        }
    }

    // Case: docs/cases/packaging.md#package-duplicate-name
    #[test]
    fn a_package_cannot_flatten_two_skills_to_the_same_name() {
        assert!(namespace("vendor", ["tdd".into(), "tdd".into()]).is_err());
        let names = namespace("vendor", ["tdd".into(), "review".into()]).unwrap();
        assert_eq!(names["review"], "vendor-review");
    }

    // Case: docs/cases/packaging.md#package-references
    #[test]
    fn reference_rewriting_preserves_other_file_extensions_and_variable_names() {
        let names = BTreeMap::from([("tdd".into(), "vendor-tdd".into())]);
        assert_eq!(
            rewrite_references("/tdd.json $tdd_options /tdd. $tdd!", &names),
            "/tdd.json $tdd_options /vendor-tdd. $vendor-tdd!"
        );
        assert_eq!(
            rewrite_references("default_prompt: \"Use $tdd\"\n", &names),
            "default_prompt: \"Use $vendor-tdd\"\n"
        );
    }

    // Case: docs/cases/packaging.md#package-references
    #[test]
    fn references_preserve_prose_paths_and_host_commands() {
        let names = BTreeMap::from([("tdd".into(), "vendor-tdd".into())]);
        assert_eq!(rewrite_references(
            "Use /tdd and $tdd. Keep /clear and /vendor-tdd.\n\nCall the Skill tool with\n\"tdd\" or `tdd`.\n\ntdd prose `tdd` ./tdd/t.md /tdd.md https://host/tdd", &names),
            "Use /vendor-tdd and $vendor-tdd. Keep /clear and /vendor-tdd.\n\nCall the Skill tool with\n\"vendor-tdd\" or `vendor-tdd`.\n\ntdd prose `tdd` ./tdd/t.md /tdd.md https://host/tdd");
    }
}
