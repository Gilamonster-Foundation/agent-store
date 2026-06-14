# agent-store build tasks.
# `just check` is the gate: it is what .githooks/pre-push and CI both run.

# Full validation: format, lint (zero warnings), tests.
check: fmt-check lint test

build:
    cargo build --all

test:
    cargo test --all

# Phase 2: exercise the Postgres backend behind its feature gate.
test-pg:
    cargo test --all --features pg

lint:
    cargo clippy --all-targets --all-features -- -D warnings

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

install-hooks:
    git config core.hooksPath .githooks
    @echo "push hooks installed (.githooks/pre-push)"

clean:
    cargo clean
