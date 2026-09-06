# Status

Current version: **0.1.2**, published on crates.io.

## Landed

- `Backend` trait + `SqliteBackend` (bundled rusqlite, zero system deps).
- `Generation` — a named monotonic counter (modulex's report clock).
- `WriterLog` — a per-`(stream, writer)` BLAKE3-chained log with
  `WriterLog::verify` (newt's ordering contract).
- `Doorbell` — the commit-notification seam consumers bridge onto agent-mesh.
- `Fingerprint` — a 32-byte BLAKE3 writer identity, wire-compatible with
  `agent-mesh-protocol`'s fingerprint derivation.
- `StorePolicy` — a declarative, config-selected backend choice
  (`backend = "sqlite" | "postgres"`), defaulting to SQLite so adopting the
  policy is a no-op until a value is deliberately set.

## Ahead

| Phase | Scope |
|-------|-------|
| **2** | `pg` feature: synchronous Postgres `Backend` (opt-in, BYO/remote server). Feature-gated but unimplemented — `StorePolicy::Postgres` is reserved, consumers reject it until this lands. |
| **3** | Operator-owned managed Postgres for in-cluster fleets. |
| **4** | Semantic recall (pgvector), in-cluster only. |

Consumers: [`modulex-mcp`](https://github.com/hartsock/modulex-mcp) has wired
its routine store onto this crate.
[`newt-agent`](https://github.com/Gilamonster-Foundation/newt-agent) is the
design target for `WriterLog` but has not yet migrated its conversation store
onto it.
