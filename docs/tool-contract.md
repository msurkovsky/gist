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

## Long-running subcommands

Some subcommands run as a host's background task instead of returning at
once: `gk md-review serve` for a whole review, `gk md-review wait` until
something happens (`docs/adr/0013`). The rules above still hold. In
addition:

- **One result on stdout.** A command that keeps running after it is ready
  (`serve`) prints its result as the first line, then nothing until it
  exits. A command that blocks for an event (`wait`) prints only its final
  result, on exit. No heartbeats, no progress, in either rendering.
- **A timeout is an outcome.** A blocking command takes `--timeout`, and it
  is required when the caller runs under a host limit. When it expires the
  command prints a timeout result in both renderings and exits `0`. The
  host kills a task that reaches its own limit and tells the agent not to
  restart it, so the command must end first; the caller then decides
  whether to run it again.
- **Restarting loses nothing.** State lives outside the process. A command
  killed, timed out or never started is safe to run again: a later `wait`
  returns what an earlier one missed, marked as a repeat when it was
  already delivered.
- **No second instance.** Starting a long-running command that is already
  running for the same target prints the running one's result and exits
  `0`; it does not start another.
- **Prompt exit on a signal.** On SIGINT or SIGTERM the command stops
  promptly and leaves its state consistent; locks are OS locks, released
  with the process.

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
