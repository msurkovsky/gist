# Working in Gist

Read `MANIFESTO.md` for the governing principles, `docs/architecture.md` for
the system, and `CONTRIBUTING.md` for the change workflow. Read `CLAUDE.md`
for repository conventions; it is shared guidance despite its filename.

Apply `rules/code-and-comments.md` when changing code and
`rules/merge-requests.md` when working on a merge request. Read
`docs/skill-contract.md` before adding or changing an owned skill, and
`docs/tool-contract.md` before changing the CLI.

Run `just ci` before finishing a change. It checks mechanical properties;
it does not establish that a skill makes good decisions. Record the relevant
behavioral evidence and any review still needed.

Never edit `experimental/`. Adopt into `skills/` or change Gist's packaging
layer. Do not install skills or hooks into a working project as a side effect
of development; integration tests use temporary directories.
