# Finding undocumented public surface

Rule 4 needs the opposite of a doc ratio: public items with *no* line at all.
These are triage greps — fast, approximate, and good enough to produce a
candidate list a human confirms. If one of them becomes load-bearing, promote
it to `gk doc --public` and delete it from here.

Each script prints `file:line: item` for a public item whose preceding line is
not documentation.

## Rust

```bash
find . -name '*.rs' -not -path '*/target/*' | xargs awk '
  /^[[:space:]]*(\/\/\/|\/\/!)/ { doc = 1; next }
  /^[[:space:]]*#\[/            { next }
  /^[[:space:]]*pub (fn|struct|enum|trait|type|const|static|mod)[ (]/ { if (!doc) print FILENAME ":" FNR ": " $0 }
  { doc = 0 }
'
```

Attributes (`#[derive(...)]`, `#[arg(...)]`) sit between the doc and the item,
so they are skipped rather than treated as a gap. A blank line does break the
association — which is correct, that detaches the doc in Rust too.

## TypeScript, JavaScript, Vue

```bash
find src -name '*.ts' -o -name '*.vue' | xargs awk '
  /\*\//                  { doc = 1; next }
  /^[[:space:]]*(@|\/\/)/ { next }
  /^export (default )?(async )?(function|const|class|interface|type|enum|abstract)/ { if (!doc) print FILENAME ":" FNR ": " $0 }
  { doc = 0 }
'
```

Only top-level exports. Decorators and line comments pass through, so
`@Component`-style code does not read as undocumented.

## Go

```bash
find . -name '*.go' | xargs awk '
  /^\/\// { doc = 1; next }
  /^(func|type|var|const) [A-Z]/ || /^func \([^)]*\) [A-Z]/ { if (!doc) print FILENAME ":" FNR ": " $0 }
  { doc = 0 }
'
```

Exported means capitalised, and the doc comment must start with the item's
name — worth checking by eye once the list is short.

## Python

The docstring comes *after* the definition, so look forward instead:

```bash
find . -name '*.py' | xargs awk '
  pending { if ($0 !~ /^[[:space:]]*("""|'"'"''"'"''"'"')/) print FILENAME ":" line ": " item; pending = 0 }
  /^(def|class) [a-zA-Z]/ { item = $0; line = FNR; pending = 1 }
'
```

Module-level only, and leading-underscore names are private by convention —
ignore them if they show up.

## Narrowing to the change

Public items that already existed are not this review's problem. Intersect with
the changed files first:

```bash
git diff --name-only --merge-base main | grep '\.rs$' | xargs awk -f /tmp/undoc.awk
```
