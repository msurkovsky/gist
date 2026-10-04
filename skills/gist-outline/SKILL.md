---
name: gist-outline
description: Summarize a codebase's file counts, dominant languages, and size when asked for an overview or when unfamiliar-tree orientation helps the requested work.
---

# Outline

Use a tree summary when it helps orient the requested work.

## Usage

```bash
gk outline <path> --json
```

Options: `--limit N` caps reported file types, `--all` includes gitignored files.
If `gk` is unavailable or the command fails, report the missing prerequisite or
error. Do not invent counts or install tools without an installation request.
When `gk` is missing, give the user its install command:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/msurkovsky/gist/releases/latest/download/gist-cli-installer.sh | sh
```

## Reading the result

- `files` / `bytes` — total size of the problem
- `file_types` — sorted by bytes, biggest footprint first
- `truncated` — true means you are seeing a slice, raise `--limit`

## What to do with it

Report the shape. Continue with the user's requested inspection when it is clear;
ask for the desired level of detail only when it would change the next step.
File types and sizes do not establish module responsibilities; inspect the
relevant files before making architectural claims.
