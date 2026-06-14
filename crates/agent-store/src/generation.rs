//! Monotonic generation counter.
//!
//! A [`Generation`] is a named, strictly-increasing `u64` kept in a small meta
//! table. It is the causal clock modulex-mcp already reasons with (its report
//! identity / `last_generation`), lifted into the shared substrate so every
//! consumer stamps rows the same way.
//!
//! **Wall-clock time is never a coordination primitive.** A generation only
//! ever moves forward; [`Generation::set`] refuses to move it backward.

use crate::backend::{as_u64, Backend, Value};
use crate::error::{Result, StoreError};

const META_TABLE: &str = "_agent_store_meta";

/// A named monotonic counter.
#[derive(Clone, Debug)]
pub struct Generation {
    key: String,
}

impl Generation {
    /// Name a counter. Many independent counters can coexist in one database.
    pub fn new(key: impl Into<String>) -> Self {
        Self { key: key.into() }
    }

    /// Create the backing meta table if it does not exist. Idempotent.
    pub fn ensure_schema(db: &dyn Backend) -> Result<()> {
        db.exec(
            &format!(
                "CREATE TABLE IF NOT EXISTS {META_TABLE} (\
                 key TEXT PRIMARY KEY, ival INTEGER NOT NULL)"
            ),
            &[],
        )?;
        Ok(())
    }

    /// The current value (0 if the counter has never been bumped).
    pub fn current(&self, db: &dyn Backend) -> Result<u64> {
        let rows = db.query(
            &format!("SELECT ival FROM {META_TABLE} WHERE key = ?"),
            &[Value::Text(self.key.clone())],
        )?;
        match rows.first() {
            Some(row) => as_u64(&row[0]),
            None => Ok(0),
        }
    }

    /// Atomically increment and return the new value. The first bump yields 1.
    pub fn bump(&self, db: &dyn Backend) -> Result<u64> {
        // One atomic upsert-with-RETURNING (supported by both SQLite ≥3.35 and
        // Postgres). `ival = ival + 1` references the existing row's value.
        let rows = db.query(
            &format!(
                "INSERT INTO {META_TABLE} (key, ival) VALUES (?, 1) \
                 ON CONFLICT(key) DO UPDATE SET ival = ival + 1 RETURNING ival"
            ),
            &[Value::Text(self.key.clone())],
        )?;
        let row = rows
            .first()
            .ok_or_else(|| StoreError::Backend("bump: RETURNING produced no row".into()))?;
        as_u64(&row[0])
    }

    /// Set the counter to an explicit value, refusing to move it backward.
    pub fn set(&self, db: &dyn Backend, value: u64) -> Result<()> {
        let current = self.current(db)?;
        if value < current {
            return Err(StoreError::NonMonotonicGeneration {
                key: self.key.clone(),
                current,
                attempted: value,
            });
        }
        db.exec(
            &format!(
                "INSERT INTO {META_TABLE} (key, ival) VALUES (?, ?) \
                 ON CONFLICT(key) DO UPDATE SET ival = excluded.ival"
            ),
            &[Value::Text(self.key.clone()), Value::Int(value as i64)],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::SqliteBackend;

    fn db() -> SqliteBackend {
        let db = SqliteBackend::in_memory().unwrap();
        Generation::ensure_schema(&db).unwrap();
        db
    }

    #[test]
    fn starts_at_zero_then_increments() {
        let db = db();
        let g = Generation::new("report");
        assert_eq!(g.current(&db).unwrap(), 0);
        assert_eq!(g.bump(&db).unwrap(), 1);
        assert_eq!(g.bump(&db).unwrap(), 2);
        assert_eq!(g.current(&db).unwrap(), 2);
    }

    #[test]
    fn counters_are_independent() {
        let db = db();
        let a = Generation::new("a");
        let b = Generation::new("b");
        a.bump(&db).unwrap();
        a.bump(&db).unwrap();
        b.bump(&db).unwrap();
        assert_eq!(a.current(&db).unwrap(), 2);
        assert_eq!(b.current(&db).unwrap(), 1);
    }

    #[test]
    fn set_advances_but_never_rewinds() {
        let db = db();
        let g = Generation::new("report");
        g.set(&db, 10).unwrap();
        assert_eq!(g.current(&db).unwrap(), 10);

        // Regression: a backward set must be refused (monotonic contract).
        let err = g.set(&db, 3).unwrap_err();
        assert!(matches!(
            err,
            StoreError::NonMonotonicGeneration {
                current: 10,
                attempted: 3,
                ..
            }
        ));
        assert_eq!(g.current(&db).unwrap(), 10);
    }
}
