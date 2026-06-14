//! Error type for the store substrate.

use thiserror::Error;

/// Errors raised by the store substrate.
///
/// Backend-specific failures (rusqlite, and later `postgres`) are flattened
/// into [`StoreError::Backend`] with a stringified detail, so callers never
/// have to match on a backend's concrete error type — the substrate is the
/// boundary.
#[derive(Debug, Error)]
pub enum StoreError {
    /// A backend (SQLite / Postgres) returned an error.
    #[error("backend error: {0}")]
    Backend(String),

    /// A [`crate::WriterLog`] chain failed verification — the log has been
    /// reordered, truncated, or tampered with.
    #[error("chain broken on stream {stream:?} writer {writer:?} at seq {seq}: {detail}")]
    ChainBroken {
        stream: String,
        writer: String,
        seq: u64,
        detail: String,
    },

    /// A generation counter was asked to move backwards. Generations are
    /// monotonic by contract — wall-clock time is never a coordination
    /// primitive here.
    #[error("non-monotonic generation on {key:?}: current {current}, attempted {attempted}")]
    NonMonotonicGeneration {
        key: String,
        current: u64,
        attempted: u64,
    },

    /// A stored value had an unexpected shape (e.g. a hash column that was
    /// not exactly 32 bytes).
    #[error("malformed row: {0}")]
    MalformedRow(String),
}

/// Convenience alias for results in this crate.
pub type Result<T> = std::result::Result<T, StoreError>;

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Backend(e.to_string())
    }
}
