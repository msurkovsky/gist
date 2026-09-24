---
name: gist-outline
description: Summarize the shape of a codebase — file counts, dominant languages, where the weight sits. Use when orienting in an unfamiliar repo, before planning work, or when asked "what is this project" or "what am I looking at".
---

# Outline

Get the shape of a tree before reading anything in it.

## Usage

```bash
gk outline <path> --json
```

Options: `--limit N` caps reported file types, `--all` includes gitignored files.

## Reading the result

- `files` / `bytes` — total size of the problem
- `file_types` — sorted by bytes, biggest footprint first
- `truncated` — true means you are seeing a slice, raise `--limit`

## What to do with it

Report the shape, then ask what altitude is wanted. Do not open files yet.
The point is to know what kind of repo this is before spending context on it.
