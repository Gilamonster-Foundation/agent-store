//! Per-writer, BLAKE3-chained append log — newt-agent's §6 ordering contract,
//! lifted into the substrate.
//!
//! Each `(stream, writer)` pair owns a strictly-monotonic `seq` starting at 1.
//! Every entry's `content_hash` chains the previous entry's hash:
//!
//! ```text
//! content_hash = BLAKE3( prev_hash ?: "" || payload )
//! ```
//!
//! so the log is tamper-evident: [`WriterLog::verify`] recomputes the chain
//! and rejects any reorder, gap, or edit. Ordering is purely causal — no
//! wall-clock anywhere.
//!
//! v0 note: `append` does read-head-then-insert; the `PRIMARY KEY(stream,
//! writer, seq)` makes a duplicate `seq` a hard error rather than silent
//! corruption. A single writer is single-threaded within a process by
//! construction (one identity per agent), which is the intended use.

use crate::backend::{as_u64, blob32, Backend, Value};
use crate::error::{Result, StoreError};

const LOG_TABLE: &str = "_agent_store_log";

/// One entry in a writer's chained log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub stream: String,
    pub writer: String,
    pub seq: u64,
    /// Previous entry's `content_hash`; `None` for the genesis entry.
    pub prev_hash: Option<[u8; 32]>,
    pub content_hash: [u8; 32],
    pub payload: Vec<u8>,
}

/// Stateless operations over the chained log table.
pub struct WriterLog;

impl WriterLog {
    /// Create the backing table if it does not exist. Idempotent.
    pub fn ensure_schema(db: &dyn Backend) -> Result<()> {
        db.exec(
            &format!(
                "CREATE TABLE IF NOT EXISTS {LOG_TABLE} (\
                 stream TEXT NOT NULL, \
                 writer TEXT NOT NULL, \
                 seq INTEGER NOT NULL, \
                 prev_hash BLOB, \
                 content_hash BLOB NOT NULL, \
                 payload BLOB NOT NULL, \
                 PRIMARY KEY (stream, writer, seq))"
            ),
            &[],
        )?;
        Ok(())
    }

    /// The highest-seq entry for `(stream, writer)`, or `None` if empty.
    pub fn head(db: &dyn Backend, stream: &str, writer: &str) -> Result<Option<Entry>> {
        let rows = db.query(
            &format!(
                "SELECT seq, prev_hash, content_hash, payload FROM {LOG_TABLE} \
                 WHERE stream = ? AND writer = ? ORDER BY seq DESC LIMIT 1"
            ),
            &[Value::Text(stream.into()), Value::Text(writer.into())],
        )?;
        match rows.first() {
            None => Ok(None),
            Some(row) => Ok(Some(row_to_entry(stream, writer, row)?)),
        }
    }

    /// Append `payload`, computing the next `seq` and chained `content_hash`.
    pub fn append(db: &dyn Backend, stream: &str, writer: &str, payload: &[u8]) -> Result<Entry> {
        let (seq, prev_hash) = match Self::head(db, stream, writer)? {
            Some(h) => (h.seq + 1, Some(h.content_hash)),
            None => (1, None),
        };
        let content_hash = chain_hash(prev_hash.as_ref(), payload);
        db.exec(
            &format!(
                "INSERT INTO {LOG_TABLE} \
                 (stream, writer, seq, prev_hash, content_hash, payload) \
                 VALUES (?, ?, ?, ?, ?, ?)"
            ),
            &[
                Value::Text(stream.into()),
                Value::Text(writer.into()),
                Value::Int(seq as i64),
                match prev_hash {
                    Some(h) => Value::Blob(h.to_vec()),
                    None => Value::Null,
                },
                Value::Blob(content_hash.to_vec()),
                Value::Blob(payload.to_vec()),
            ],
        )?;
        Ok(Entry {
            stream: stream.into(),
            writer: writer.into(),
            seq,
            prev_hash,
            content_hash,
            payload: payload.to_vec(),
        })
    }

    /// All entries for `(stream, writer)`, ascending by seq.
    pub fn entries(db: &dyn Backend, stream: &str, writer: &str) -> Result<Vec<Entry>> {
        let rows = db.query(
            &format!(
                "SELECT seq, prev_hash, content_hash, payload FROM {LOG_TABLE} \
                 WHERE stream = ? AND writer = ? ORDER BY seq ASC"
            ),
            &[Value::Text(stream.into()), Value::Text(writer.into())],
        )?;
        rows.iter()
            .map(|r| row_to_entry(stream, writer, r))
            .collect()
    }

    /// Recompute the chain and reject any gap, reorder, or tamper.
    pub fn verify(db: &dyn Backend, stream: &str, writer: &str) -> Result<()> {
        let mut prev: Option<[u8; 32]> = None;
        // seq is 1-based and contiguous; zip against the natural counter.
        for (expected_seq, entry) in (1u64..).zip(Self::entries(db, stream, writer)?) {
            if entry.seq != expected_seq {
                return Err(StoreError::ChainBroken {
                    stream: stream.into(),
                    writer: writer.into(),
                    seq: entry.seq,
                    detail: format!("expected seq {expected_seq}, found {}", entry.seq),
                });
            }
            if entry.prev_hash != prev {
                return Err(StoreError::ChainBroken {
                    stream: stream.into(),
                    writer: writer.into(),
                    seq: entry.seq,
                    detail: "prev_hash does not link to the prior entry".into(),
                });
            }
            let recomputed = chain_hash(prev.as_ref(), &entry.payload);
            if recomputed != entry.content_hash {
                return Err(StoreError::ChainBroken {
                    stream: stream.into(),
                    writer: writer.into(),
                    seq: entry.seq,
                    detail: "content_hash does not match payload (tampered)".into(),
                });
            }
            prev = Some(entry.content_hash);
        }
        Ok(())
    }
}

/// `BLAKE3(prev_hash? || payload)`.
fn chain_hash(prev: Option<&[u8; 32]>, payload: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    if let Some(p) = prev {
        hasher.update(p);
    }
    hasher.update(payload);
    *hasher.finalize().as_bytes()
}

fn row_to_entry(stream: &str, writer: &str, row: &[Value]) -> Result<Entry> {
    let seq = as_u64(&row[0])?;
    let prev_hash = match &row[1] {
        Value::Null => None,
        other => Some(blob32(other)?),
    };
    let content_hash = blob32(&row[2])?;
    let payload = match &row[3] {
        Value::Blob(b) => b.clone(),
        other => {
            return Err(StoreError::MalformedRow(format!(
                "payload must be a blob, got {other:?}"
            )))
        }
    };
    Ok(Entry {
        stream: stream.into(),
        writer: writer.into(),
        seq,
        prev_hash,
        content_hash,
        payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::SqliteBackend;

    fn db() -> SqliteBackend {
        let db = SqliteBackend::in_memory().unwrap();
        WriterLog::ensure_schema(&db).unwrap();
        db
    }

    #[test]
    fn appends_chain_and_verify() {
        let db = db();
        let e1 = WriterLog::append(&db, "conv:x", "alice", b"hello").unwrap();
        let e2 = WriterLog::append(&db, "conv:x", "alice", b"world").unwrap();
        assert_eq!(e1.seq, 1);
        assert_eq!(e1.prev_hash, None);
        assert_eq!(e2.seq, 2);
        assert_eq!(e2.prev_hash, Some(e1.content_hash));
        assert_ne!(e1.content_hash, e2.content_hash);
        WriterLog::verify(&db, "conv:x", "alice").unwrap();
    }

    #[test]
    fn writers_have_independent_sequences() {
        let db = db();
        WriterLog::append(&db, "conv:x", "alice", b"a").unwrap();
        let bob1 = WriterLog::append(&db, "conv:x", "bob", b"b").unwrap();
        assert_eq!(bob1.seq, 1, "each writer's seq starts at 1");
        WriterLog::verify(&db, "conv:x", "alice").unwrap();
        WriterLog::verify(&db, "conv:x", "bob").unwrap();
    }

    #[test]
    fn verify_detects_tampering() {
        // Regression: editing a stored payload must break verification, even
        // though the row still parses and the seq is intact.
        let db = db();
        WriterLog::append(&db, "conv:x", "alice", b"original").unwrap();
        WriterLog::append(&db, "conv:x", "alice", b"second").unwrap();
        // Tamper with seq 1's payload directly, behind the chain's back.
        db.exec(
            "UPDATE _agent_store_log SET payload = ? WHERE stream = ? AND writer = ? AND seq = 1",
            &[
                Value::Blob(b"TAMPERED".to_vec()),
                Value::Text("conv:x".into()),
                Value::Text("alice".into()),
            ],
        )
        .unwrap();
        let err = WriterLog::verify(&db, "conv:x", "alice").unwrap_err();
        assert!(matches!(err, StoreError::ChainBroken { seq: 1, .. }));
    }

    #[test]
    fn verify_empty_log_is_ok() {
        let db = db();
        WriterLog::verify(&db, "conv:none", "nobody").unwrap();
    }
}
