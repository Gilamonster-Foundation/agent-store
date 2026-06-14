//! # agent-store
//!
//! A causal-ordered, backend-pluggable **store substrate** for the agent
//! fleet. It is not a database and not an agent — it is the thin layer that
//! gives every consumer (newt-agent's conversation store, modulex-mcp's
//! routine board) the same three things:
//!
//! - a pluggable [`Backend`] — [`SqliteBackend`] today (bundled rusqlite,
//!   zero system deps, the laptop default); a synchronous Postgres backend
//!   behind the `pg` feature in Phase 2, for where an operator already runs a
//!   server (your own box, or airship's "Meat Locker").
//! - two causal primitives — a monotonic [`Generation`] counter (modulex's
//!   report clock) and a per-writer, BLAKE3-chained [`WriterLog`] with
//!   tamper-evident [`WriterLog::verify`] (newt's §6 ordering contract).
//! - a commit [`Doorbell`] — the seam co-located agents use to wake each
//!   other over agent-mesh instead of polling a file.
//!
//! Everything here is **synchronous**, so consumers drop it into their
//! existing call sites without an async refactor. The substrate never touches
//! the mesh and never reads the wall clock: ordering is `(writer, seq)` and
//! generation counters, never timestamps.
//!
//! ## Usage
//!
//! ```
//! use agent_store::{Doorbell, CommitEvent, Generation, SqliteBackend, WriterLog};
//!
//! // Open an ephemeral store and lay down the substrate tables.
//! let db = SqliteBackend::in_memory().unwrap();
//! Generation::ensure_schema(&db).unwrap();
//! WriterLog::ensure_schema(&db).unwrap();
//!
//! // A monotonic generation counter (modulex's report clock).
//! let report = Generation::new("report");
//! assert_eq!(report.bump(&db).unwrap(), 1);
//! assert_eq!(report.bump(&db).unwrap(), 2);
//!
//! // A per-writer chained log (newt's conversation turns), with a doorbell
//! // that a session loop would bridge onto the mesh.
//! let bell = Doorbell::new();
//! bell.subscribe(|e: &CommitEvent| {
//!     // a real consumer publishes (writer, seq) on a mesh topic here
//!     let _ = e.seq;
//! });
//!
//! let turn = WriterLog::append(&db, "conv:demo", "alice", b"hello world").unwrap();
//! bell.ring(&CommitEvent {
//!     stream: turn.stream.clone(),
//!     writer: turn.writer.clone(),
//!     seq: turn.seq,
//!     content_hash: turn.content_hash,
//! });
//!
//! // The chain is tamper-evident.
//! WriterLog::verify(&db, "conv:demo", "alice").unwrap();
//! ```

mod backend;
mod doorbell;
mod error;
mod fingerprint;
mod generation;
mod policy;
mod writer_log;

pub use backend::{Backend, Dialect, Row, SqliteBackend, Value};
pub use doorbell::{CommitEvent, Doorbell};
pub use error::{Result, StoreError};
pub use fingerprint::Fingerprint;
pub use generation::Generation;
pub use policy::{BackendKind, StorePolicy};
pub use writer_log::{Entry, WriterLog};
