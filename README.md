# agent-store

> The fleet's **causal-ordered, backend-pluggable store substrate** — one thin
> layer that gives every agent the same durable ordering, identity, and
> coordination seam, on SQLite today and PostgreSQL tomorrow.

**Status:** bootstrapping (2026-06-13)

## What this is

`agent-store` is not a database and not an agent. It is the small Rust crate
that two very different consumers sit on top of without re-inventing the same
machinery:

- [`newt-agent`](https://github.com/Gilamonster-Foundation/newt-agent)'s
  conversation store — a per-writer, BLAKE3-chained, tamper-evident log.
- [`modulex-mcp`](https://github.com/hartsock/modulex-mcp)'s routine engine —
  a monotonic generation counter that stamps every report.

It provides three things and nothing more:

1. **A pluggable [`Backend`]** — `SqliteBackend` today (bundled rusqlite, zero
   system dependencies, the laptop default). A **synchronous** Postgres backend
   lands behind the `pg` feature in Phase 2, for where an operator already runs
   a server (your own box, or [`airship`](https://github.com/Gilamonster-Foundation/airship)'s
   "Meat Locker"). Everything is synchronous, so consumers drop it into their
   existing call sites with no async refactor.
2. **Two causal primitives** — `Generation` (a named monotonic counter) and
   `WriterLog` (a per-`(stream, writer)` log whose every entry chains the
   previous entry's BLAKE3 hash, verifiable with `WriterLog::verify`).
3. **A commit [`Doorbell`]** — the seam co-located agents use to wake each
   other. The substrate only *emits* a `CommitEvent`; a consumer bridges it
   onto [`agent-mesh`](https://github.com/Gilamonster-Foundation/agent-mesh),
   publishing the causal pointer `(writer, seq)` on a per-stream topic. The
   mesh stays the only distributed fabric — `agent-store` never becomes a
   cross-machine coordination primitive.

## Load-bearing principles

- **Wall-clock time is never a coordination primitive.** Ordering is
  `(writer, seq)` and generation counters. Any timestamp a consumer stores is a
  display *claim*, never compared to make a decision.
- **The substrate is synchronous and mesh-agnostic.** No async creep, no
  dependency on the mesh transport. The doorbell hands events *out*; consumers
  do the bridging.
- **The default is a file.** SQLite, bundled, no server, no daemon. Postgres is
  opt-in and only ever a server someone else already owns and reaps.

## Usage

```rust
use agent_store::{Doorbell, CommitEvent, Generation, SqliteBackend, WriterLog};

// Open a store and lay down the substrate tables.
let db = SqliteBackend::in_memory()?;          // or SqliteBackend::open(path)?
Generation::ensure_schema(&db)?;
WriterLog::ensure_schema(&db)?;

// A monotonic generation counter (modulex's report clock).
let report = Generation::new("report");
assert_eq!(report.bump(&db)?, 1);
assert_eq!(report.bump(&db)?, 2);

// A per-writer chained log (newt's conversation turns) + a doorbell a
// session loop would bridge onto the mesh.
let bell = Doorbell::new();
bell.subscribe(|e: &CommitEvent| {
    // publish (e.writer, e.seq) on a mesh topic here
});

let turn = WriterLog::append(&db, "conv:demo", "alice", b"hello world")?;
bell.ring(&CommitEvent {
    stream: turn.stream.clone(),
    writer: turn.writer.clone(),
    seq: turn.seq,
    content_hash: turn.content_hash,
});

// The chain is tamper-evident.
WriterLog::verify(&db, "conv:demo", "alice")?;
# Ok::<(), agent_store::StoreError>(())
```

## Build & validate

```bash
just check          # fmt --check + clippy -D warnings + tests (the gate)
just install-hooks  # REQUIRED after clone: installs .githooks/pre-push
```

Zero-warnings policy; `.githooks/pre-push` and `.github/workflows/ci.yml` run
the same steps and must stay in parity.

## Roadmap

| Phase | Scope |
|-------|-------|
| **0 (this repo)** | Substrate: `Backend` + SQLite, `Generation`, `WriterLog`, `Doorbell`. |
| **1** | Consumers wire the doorbell over agent-mesh for local multi-agent coordination. |
| **2** | `pg` feature: synchronous Postgres `Backend` (opt-in, BYO/remote server). |
| **3** | airship "Meat Locker" — operator-owned managed Postgres for in-cluster fleets. |
| **4** | Semantic recall (pgvector), in-cluster only. |

## License

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
