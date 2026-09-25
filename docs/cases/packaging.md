# Packaging cases

## package-references

When installing a package containing `tdd` and `codebase-design`, both names acquire
the package prefix, and a `tdd` instruction calling `codebase-design` resolves to the
prefixed dependency. Rewrite supported `/skill`, `$skill`, and quoted `Skill tool`
references in Markdown and YAML resources as well as the entry point. Preserve
ordinary prose, local file paths, unknown host commands, and already-prefixed names.
The source files remain unchanged and both target hosts receive identical content.

## package-duplicate-name

Two skill directories in one package with the same basename cannot be flattened to
the same installed name. Refuse before writing any target content.

The unit test `a_package_cannot_flatten_two_skills_to_the_same_name` in
[`src/skill.rs`](../../tools/crates/gist-cli/src/skill.rs) checks duplicate-name
rejection and distinct-name acceptance. `collect_items` in
[`src/init.rs`](../../tools/crates/gist-cli/src/init.rs) calls this validation
before target preparation or writes. The unit test covers rejection; the ordering
is checked by code review, not by an installation fixture.

## package-line-endings

Reference rewriting treats LF and CRLF blank lines, including whitespace-only
blank lines, as paragraph boundaries. A `Skill tool` invocation in one paragraph
does not change ordinary quoted names in another. Preserve the original separators.
Frontmatter name rewriting changes only the name line's content, preserving all
original line endings, even mixed endings, and the presence or absence of a final newline.
