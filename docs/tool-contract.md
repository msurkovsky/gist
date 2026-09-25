# Tool contract

Every `gk` subcommand is called by an agent more often than by a person.
These rules exist so the agent never has to guess.

## Output

- `--json` emits an envelope: `{"status":"ok","data":{...}}` on success,
  `{"status":"error","message":"..."}` on failure. Errors go to stderr.
- Without `--json`, emit plain text for a human reading a terminal.
- Both renderings are mandatory. `gist_core::Human` enforces it at compile time.

## Exit codes

| code | meaning |
|------|---------|
| `0`  | did what was asked |
| `1`  | expected failure — nothing found, path missing, work refused |
| `2`  | called wrong — bad flags, unparseable input (clap returns this) |

A run that completed but left something refused, such as an `init` conflict or a
`hook install` that found a hook that is not gk's, or a `check` report with validation
failures, still prints its report
with `"status":"ok"` on stdout and exits `1`. The report says what; the exit code
says look before continuing.

## Behavior

- Never interactive. No prompts, no confirmations, no spinners.
- Never unbounded. Anything that can produce a long list takes `--limit` and
  reports `truncated` so the caller knows it is seeing a slice. `gk init` is the
  exception and takes no `--limit`: what it lists is fixed by the embedded skills
  plus the `--experimental` packages the caller names, not by the caller's data.
  Installing a foreign package is the maintainer's manual call, so its size is
  theirs to judge.
- Never chatty. No progress logs on stdout; stdout is the result, nothing else.
  `gk hook commit-msg` prints its advice on stderr and leaves stdout empty, since
  git shows a hook's output to the committer on every commit (`docs/adr/0009`).
- Deterministic ordering. Sort explicitly — hash iteration order is not an order.

## Adding a subcommand

Follow the approved-prose workflow in `CONTRIBUTING.md`. Add behavior cases under
`docs/cases/` and reference them from the tests that exercise the changed boundary.

1. Add a variant to `Command` in `crates/gist-cli/src/main.rs`.
2. Define a `#[derive(Debug, Serialize)]` result type and `impl Human` for it.
3. Return `Result<T, String>`; the error string is what the caller sees.
4. Test the failure path, not just the happy one.
5. Add an end-to-end test in `crates/gist-cli/tests/cli.rs`. Unit tests cover
   classification; only an end-to-end test covers exit codes, the envelope,
   and whatever git actually does.
