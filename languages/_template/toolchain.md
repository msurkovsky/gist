# Toolchain adapter: <language>

Copy this directory to `languages/<lang>/` and fill every section. The `toolchain` skill
reads only this file for language-specific commands. A verb left as `none` is reported as
`unsupported verb` by the skill; a missing file is reported as `unsupported language`.
Never leave a guess in here: a wrong command is worse than `none`.

## Detect

Manifest files whose presence in a folder means this language applies to that folder group.
One per line, exact file names or globs.

```
<manifest>          # e.g. a build or dependency manifest at the folder root
```

## Verbs

Each verb: the command to run from the folder group root, then how to read the result.
Project overrides in the project's `CLAUDE.md` or `.claude/rules/` win over these defaults.

### install

Bring dependencies and generated code in line with the lockfile. Run after every rebase.

```
none
```

Read: exit code only.

### typecheck

```
none
```

Read: exit code; non-zero means fail. Paste only the first error block.

### lint

```
none
```

Read: exit code; list findings as `file:line message`.

### test

Full suite, non-interactive, no watch mode.

```
none
```

Read: exit code; on failure list failing test names only.

### coverage

Must write a machine-readable report in **LCOV** (`lcov.info`) or **Cobertura XML**. The
diff-coverage step in the `toolchain` skill is language-free and only understands those two.

```
none
```

Report path: `<path relative to folder group root>`

### dead-code

Unused exports, unreachable branches, unused dependencies. Output one finding per line.

```
none
```

Read: `file:line symbol`.

## Prove-red notes

How to run a single test file or test name in isolation, used to prove a new test fails
without its fix:

```
none
```

## Known pitfalls

Anything that makes a green run untrustworthy in this language, for example stale build
caches or generated clients that shadow hand-written types.
