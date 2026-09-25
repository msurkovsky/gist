# Views

One directory per consumer, each holding a `workspace.josh`. A view is a Josh workspace: a
small virtual repository composed from paths in this central repo, cloned wherever the
consumer expects skills, with edits flowing back through `josh push`.

Create the directory and file here, commit, then:

```
josh clone <this repo> ':workspace=views/<consumer>' <destination>
```

Mapping syntax, one per line, `destination = :/source/path`:

```
gist-outline = :/skills/gist-outline
tdd = :/experimental/mattpocock/skills/engineering/tdd
```

Josh rewrites the file into a canonical nested form on the first push. That is expected.

No views are defined yet. Suggested first one: `claude` for `~/.claude/skills` on this
machine.
