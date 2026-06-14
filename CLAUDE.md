# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What agent-store is

The fleet's causal-ordered, backend-pluggable **store substrate**. Not a
database, not an agent — the thin layer newt-agent's conversation store and
modulex-mcp's routine engine both sit on, so neither re-invents monotonic
ordering, content-chaining, or the local-coordination seam.

It is a telescope, not the sky: the point is *durable causal ordering and
local coordination*, identically for every consumer. Don't grow consumer
features into the substrate — grow primitives and backends.

## Hard rules

1. **Wall-clock time is never a coordination primitive.** Ordering is
   `(writer, seq)` and generation counters. A stored timestamp is a display
   *claim* only, never compared to make a decision. No `SystemTime::now()` /
   `Instant` in ordering logic.
2. **The substrate is synchronous.** Both consumers call sync rusqlite from
   async contexts today; keeping the trait sync lets them drop us in with no
   async refactor. The Phase-2 Postgres backend uses the **synchronous**
   `postgres` crate (interior mutability behind `&self`), **not** async `sqlx`.
   No `async fn` in this crate.
3. **The substrate is mesh-agnostic.** `agent-store` never depends on
   agent-mesh transport and never opens a socket. The `Doorbell` only *emits*
   `CommitEvent`s; consumers bridge them onto the mesh. `agent-store` must
   never become a cross-machine coordination primitive — that stays the mesh.
4. **Backend-neutral SQL.** Primitives write the portable subset both SQLite
   and Postgres share, with `?` positional placeholders (the Postgres backend
   rewrites `?` → `$n`). Dialect-specific SQL goes behind `Backend::dialect()`.
5. **No daemons, no server-on-a-laptop.** The default backend is a bundled
   SQLite file. Postgres is opt-in and only ever a server an operator already
   runs and reaps. Nothing here starts or `initdb`s a database.

## Build & validate

```bash
just check          # fmt --check + clippy -D warnings + tests (the gate)
just install-hooks  # REQUIRED after clone: installs .githooks/pre-push
```

Zero-warnings policy: `cargo clippy --all-targets --all-features -- -D warnings`
must be clean before any push.

## Hook / pipeline parity

`.githooks/pre-push` and `.github/workflows/ci.yml` must run the same steps.
Editing either REQUIRES auditing the other. Both carry cross-reference
comments — keep them.

## Workflow

- Branch → TDD → `just check` green → push → PR → human merges. Agents do not
  push to main and do not merge.
- Every bug fix includes a regression test that fails on the old code.
- One logical change per branch; branches live hours-to-days, not weeks.

## Versioning

- **0.1.x semver line** (Shawn, 2026-06-13). First publish = `0.1.0`, cut
  manually after newt-agent and modulex-mcp consume it in the field. All crates
  lock-step in `[workspace.package]`.

## Constellation

- [`agent-mesh`](https://github.com/Gilamonster-Foundation/agent-mesh) — the
  distributed fabric (the doorbell bridges onto it; we never embed it).
- [`newt-agent`](https://github.com/Gilamonster-Foundation/newt-agent) —
  first consumer: conversation store on `WriterLog`.
- [`modulex-mcp`](https://github.com/hartsock/modulex-mcp) — concurrent
  consumer: routine clock on `Generation`.
- An operator-owned managed Postgres "Meat Locker" — the in-cluster carrier
  backend (Phase 3).

`agent-store` is the concrete realization of the long-planned `agent-mesh-store`
idea: shared durable causal state, backing-store-independent.
