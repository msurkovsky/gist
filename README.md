# Gist

*Understand enough to steer.*

AI writes the code. You still own the call.

Gist is a set of skills and tooling for developers working this way — where the
model is the doer and the human stays at the altitude where judgment still
matters. Not line-by-line review. Enough understanding to participate: to scope
the work, read the shape of what came back, catch the wrong turn, and decide.

## Layout

```
skills/     the product — one directory per skill, embedded into gk
tools/      cargo workspace; builds the gk binary
docs/       tool contract, and decisions in docs/adr
```

## Build

```bash
just ci        # fmt, clippy, test
just install   # put gk on PATH
```

## Use

```bash
gk outline .        # what shape is this repo
gk doc              # how much of my change is documentation
gk init --claude    # vendor the skills into ./.claude/skills/ of the repo you are in
```

`init` is idempotent and never overwrites a file you edited — it reports a
conflict instead; `--force` overrides. Skills land as `gist-outline` and
`gist-doc-review`, prefixed so they cannot shadow one the target repo already
has.

## Status

Early. Two skills, three tools, and a contract to keep the rest honest.
