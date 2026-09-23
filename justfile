# Gist — development tasks

tools := "tools"

default:
    @just --list

build:
    cargo build --manifest-path {{tools}}/Cargo.toml

release:
    cargo build --release --manifest-path {{tools}}/Cargo.toml

test:
    cargo test --manifest-path {{tools}}/Cargo.toml

fmt:
    cargo fmt --manifest-path {{tools}}/Cargo.toml

lint:
    cargo fmt --manifest-path {{tools}}/Cargo.toml --check
    cargo clippy --manifest-path {{tools}}/Cargo.toml --all-targets -- -D warnings

# Everything CI runs, locally.
ci: lint test

# Put gk on PATH.
install:
    cargo install --path {{tools}}/crates/gist-cli
