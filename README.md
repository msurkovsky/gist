# Gist

*Understand enough to steer.*

AI writes the code. You still own the call.

Gist is a set of skills and tooling for developers working this way — where the
model is the doer and the human stays at the altitude where judgment still
matters. Not line-by-line review. Enough understanding to participate: to scope
the work, read the shape of what came back, catch the wrong turn, and decide.

## Layout

```
skills/     the product — one directory per skill
tools/      cargo workspace; builds the gk binary
docs/       tool contract and design notes
```

## Build

```bash
just ci        # fmt, clippy, test
just install   # put gk on PATH
gk outline .   # what shape is this repo
gk doc         # how much of my change is documentation
```

## Status

Early. Two skills, two tools, and a contract to keep the rest honest.
