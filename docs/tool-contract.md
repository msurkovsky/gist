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

## Behavior

- Never interactive. No prompts, no confirmations, no spinners.
- Never unbounded. Anything that can produce a long list takes `--limit` and
  reports `truncated` so the caller knows it is seeing a slice.
- Never chatty. No progress logs on stdout; stdout is the result, nothing else.
- Deterministic ordering. Sort explicitly — hash iteration order is not an order.

## Adding a subcommand

1. Add a variant to `Command` in `crates/gist-cli/src/main.rs`.
2. Define a `#[derive(Debug, Serialize)]` result type and `impl Human` for it.
3. Return `Result<T, String>`; the error string is what the caller sees.
4. Test the failure path, not just the happy one.
5. Add an end-to-end test in `crates/gist-cli/tests/cli.rs`. Unit tests cover
   classification; only an end-to-end test covers exit codes, the envelope,
   and whatever git actually does.
