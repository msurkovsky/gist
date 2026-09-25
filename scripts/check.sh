#!/usr/bin/env bash
# Use the checkout's binary, never an installed copy with stale checks.
exec cargo run --quiet --locked --manifest-path "$(dirname "$0")/../tools/Cargo.toml" --bin gk -- check "$(dirname "$0")/.."
