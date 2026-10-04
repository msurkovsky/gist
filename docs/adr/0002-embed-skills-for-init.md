# 2. Embed skills for `init`

Accepted — 2026-09-24.

## Context

`gk init --claude` needs `skills/*` at runtime in an arbitrary target repo,
from a binary installed via `cargo install --path` (`justfile`'s `install`
recipe), which carries no source tree alongside it.

## Options

### Require a gist checkout at runtime

Dismissed. Defeats the purpose — the point of `init --claude` is running it
from *other* projects that have no gist checkout at all.

### A separate post-install data directory

Dismissed. `cargo install` has no post-install hook, so this would need a
whole separate installer and an XDG-style convention just to place three
files.

### Hand-written `include_str!` per file

Dismissed. This is what `include_dir!` already does, automatically, without
needing a human or build script to update a file list every time a skill file
is added or removed.

### `include_dir!` (chosen)

`include_dir` (0.7) embeds a whole directory tree at compile time as a
`Dir`/`File` structure, walked the same way as a filesystem `Dir` would be —
no runtime access to the source tree needed.

## Decision

Embed `skills/` via `include_dir!("$CARGO_MANIFEST_DIR/../../../skills")` in
`crates/gist-cli/src/init.rs`. `experimental/` is embedded the same way for
`--experimental` (`docs/adr/0005`), only under the `experimental` cargo feature,
which release builds leave off. `crates/gist-cli/build.rs` tells cargo to
rebuild when either tree gains or loses a file, which `include_dir!` cannot
report itself.

## Consequences

Binary size grows with `skills/` (negligible today). `skills/` becomes a
compile-time input — editing a skill needs a rebuild and reinstall of `gk`
before `init --claude` reflects it elsewhere. Deliberately no "read from disk
if inside the gist repo" fallback: one code path, not two that could drift. A
stale binary vendoring into an already-updated target reports `conflict`
rather than `unchanged` — not a new failure mode, just a "did you rebuild"
gotcha worth remembering.

## Verification

`the_embedded_skills_are_present_and_named_with_the_gist_prefix` (in
`init.rs`) fails the build immediately if `skills/` moves or empties out.

## Changelog

| When | Who | Why |
|---|---|---|
| 2026-10-04 21:23 | Martin Surkovsky | Release binaries leave `experimental/` out, so only a contributor build embeds it |
| 2026-09-25 19:00 | Martin Surkovsky | Moved the `experimental/` and `build.rs` note from the header into the Decision |
| 2026-09-25 07:19 | Martin Surkovsky | Noted that ADR 0005 embeds `experimental/` the same way, so this no longer reads as skills-only |
| 2026-09-24 07:20 | Martin Surkovsky | Created |
