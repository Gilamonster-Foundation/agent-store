//! The pluggable storage backend.
//!
//! The substrate's primitives ([`crate::Generation`], [`crate::WriterLog`])
//! are written against the [`Backend`] trait, never against a concrete
//! database. Today the only implementation is [`SqliteBackend`] (rusqlite,
//! bundled — zero system deps, the fleet default). A `PgBackend` over the
//! **synchronous** `postgres` crate slots in behind the same trait in Phase 2
//! without touching any primitive code; it will wrap its client in interior
//! mutability so the `&self` shape here still holds.
//!
//! SQL is written with `?` positional placeholders. SQLite consumes them
//! directly; the future Postgres backend rewrites `?` → `$1, $2, …`. Keep the
//! SQL to the portable subset both dialects share (the primitives do).

use std::path::Path;

use crate::error::{Result, StoreError};

/// A database-neutral value, used for both bind parameters and returned cells.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

/// One returned row: a column-ordered vector of [`Value`]s.
pub type Row = Vec<Value>;

/// Which SQL dialect a backend speaks — primitives use this only for the few
/// places the portable subset is not enough.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    Sqlite,
    Postgres,
}

/// A pluggable, **synchronous** storage backend.
///
/// Object-safe on purpose: primitives take `&dyn Backend`, so a single
/// compiled primitive serves every backend.
pub trait Backend {
    /// The dialect this backend speaks.
    fn dialect(&self) -> Dialect;

    /// Run a statement, returning the number of rows affected.
    fn exec(&self, sql: &str, params: &[Value]) -> Result<u64>;

    /// Run a query, returning all rows. Also used for `RETURNING` statements.
    fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>>;
}

// ---------------------------------------------------------------------------
// SQLite backend (the fleet default)
// ---------------------------------------------------------------------------

/// A SQLite-backed [`Backend`] using bundled rusqlite (no system libsqlite3).
pub struct SqliteBackend {
    conn: rusqlite::Connection,
}

impl SqliteBackend {
    /// Open (or create) a SQLite database at `path`, in WAL mode with a busy
    /// timeout so co-located processes serialize cleanly rather than erroring.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = rusqlite::Connection::open(path)?;
        Self::apply_pragmas(&conn)?;
        Ok(Self { conn })
    }

    /// Open an in-memory database (tests, ephemeral use).
    pub fn in_memory() -> Result<Self> {
        let conn = rusqlite::Connection::open_in_memory()?;
        Ok(Self { conn })
    }

    /// Wrap a connection a consumer already owns — the **incremental-adoption
    /// seam**. A consumer (newt's `ConversationStore`, modulex's `Store`) that
    /// already holds a `rusqlite::Connection` hands it over, keeps running its
    /// own domain SQL through [`SqliteBackend::connection`], and gets the
    /// agent-store primitives on the *same* database: no second connection, no
    /// big-bang rewrite. Pragmas are the caller's responsibility here (the
    /// connection is assumed already configured).
    pub fn from_connection(conn: rusqlite::Connection) -> Self {
        Self { conn }
    }

    /// Borrow the underlying SQLite connection for backend-specific
    /// (domain-table) SQL. SQLite-only by nature — the [`Backend`] trait stays
    /// the portable, backend-agnostic surface; this escape hatch is how a
    /// consumer keeps its existing rusqlite code while adopting the substrate.
    pub fn connection(&self) -> &rusqlite::Connection {
        &self.conn
    }

    fn apply_pragmas(conn: &rusqlite::Connection) -> Result<()> {
        // WAL + a generous busy timeout: multiple co-located agents serialize
        // on the write lock instead of failing fast. (NFS-home degradation to
        // DELETE journal mode is a consumer concern, handled where the path is
        // chosen, not here.)
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(())
    }
}

impl Backend for SqliteBackend {
    fn dialect(&self) -> Dialect {
        Dialect::Sqlite
    }

    fn exec(&self, sql: &str, params: &[Value]) -> Result<u64> {
        let n = self
            .conn
            .execute(sql, rusqlite::params_from_iter(params.iter()))?;
        Ok(n as u64)
    }

    fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>> {
        let mut stmt = self.conn.prepare(sql)?;
        let ncols = stmt.column_count();
        let rows = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), |row| {
                (0..ncols)
                    .map(|i| row.get_ref(i).map(value_from_ref))
                    .collect::<rusqlite::Result<Row>>()
            })?
            .collect::<rusqlite::Result<Vec<Row>>>()?;
        Ok(rows)
    }
}

fn value_from_ref(v: rusqlite::types::ValueRef<'_>) -> Value {
    use rusqlite::types::ValueRef;
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::Int(i),
        ValueRef::Real(f) => Value::Real(f),
        ValueRef::Text(t) => Value::Text(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => Value::Blob(b.to_vec()),
    }
}

impl rusqlite::ToSql for Value {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        use rusqlite::types::{ToSqlOutput, Value as SqlValue, ValueRef};
        Ok(match self {
            Value::Null => ToSqlOutput::Owned(SqlValue::Null),
            Value::Int(i) => ToSqlOutput::Owned(SqlValue::Integer(*i)),
            Value::Real(f) => ToSqlOutput::Owned(SqlValue::Real(*f)),
            Value::Text(s) => ToSqlOutput::Borrowed(ValueRef::Text(s.as_bytes())),
            Value::Blob(b) => ToSqlOutput::Borrowed(ValueRef::Blob(b)),
        })
    }
}

/// Extract exactly 32 bytes from a `Value::Blob` (hash columns).
pub(crate) fn blob32(v: &Value) -> Result<[u8; 32]> {
    match v {
        Value::Blob(b) if b.len() == 32 => {
            let mut out = [0u8; 32];
            out.copy_from_slice(b);
            Ok(out)
        }
        Value::Blob(b) => Err(StoreError::MalformedRow(format!(
            "expected 32-byte hash, got {} bytes",
            b.len()
        ))),
        other => Err(StoreError::MalformedRow(format!(
            "expected blob hash, got {other:?}"
        ))),
    }
}

/// Extract a `u64` from a `Value::Int`.
pub(crate) fn as_u64(v: &Value) -> Result<u64> {
    match v {
        Value::Int(i) => Ok(*i as u64),
        other => Err(StoreError::MalformedRow(format!(
            "expected integer, got {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_values() {
        let db = SqliteBackend::in_memory().unwrap();
        db.exec(
            "CREATE TABLE t (i INTEGER, r REAL, s TEXT, b BLOB, n INTEGER)",
            &[],
        )
        .unwrap();
        db.exec(
            "INSERT INTO t (i, r, s, b, n) VALUES (?, ?, ?, ?, ?)",
            &[
                Value::Int(42),
                Value::Real(1.5),
                Value::Text("hi".into()),
                Value::Blob(vec![1, 2, 3]),
                Value::Null,
            ],
        )
        .unwrap();
        let rows = db.query("SELECT i, r, s, b, n FROM t", &[]).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0],
            vec![
                Value::Int(42),
                Value::Real(1.5),
                Value::Text("hi".into()),
                Value::Blob(vec![1, 2, 3]),
                Value::Null,
            ]
        );
    }

    #[test]
    fn dialect_is_sqlite() {
        let db = SqliteBackend::in_memory().unwrap();
        assert_eq!(db.dialect(), Dialect::Sqlite);
    }

    #[test]
    fn from_connection_wraps_and_shares_the_database() {
        // A consumer's own connection, handed to the substrate.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let db = SqliteBackend::from_connection(conn);

        // Substrate writes through the Backend trait...
        db.exec("CREATE TABLE t (x INTEGER)", &[]).unwrap();
        // ...and the consumer keeps its own rusqlite domain SQL via the escape
        // hatch — both hit the same database.
        db.connection()
            .execute("INSERT INTO t VALUES (7)", [])
            .unwrap();

        let rows = db.query("SELECT x FROM t", &[]).unwrap();
        assert_eq!(rows, vec![vec![Value::Int(7)]]);
    }
}
