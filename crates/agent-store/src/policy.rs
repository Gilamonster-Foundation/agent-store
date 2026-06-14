//! Declarative storage policy.
//!
//! The backend a consumer opens is a **reviewed configuration choice**, never a
//! hardcoded commitment in code. [`StorePolicy`] is that seam: its [`Default`]
//! is today's behavior — a local SQLite file, no daemon — so adopting a policy
//! changes nothing until a value is deliberately set, and changing direction
//! (to Postgres, later) is a config edit rather than a rewrite.
//!
//! The policy is **flat by design**: one reviewable knob per line, embedded in
//! a consumer's existing config (`[store] backend = "sqlite"`). This crate owns
//! only the vocabulary — *resolution* lives in each consumer, because how a
//! backend opens (pragmas, connection ownership, domain-specific SQL) is
//! consumer-specific.

use serde::{Deserialize, Serialize};

/// Which storage backend a [`StorePolicy`] selects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendKind {
    /// Bundled SQLite — zero system deps, no daemon. The fleet default.
    #[default]
    Sqlite,
    /// PostgreSQL — opt-in, only where an operator already runs a server.
    /// Reserved: consumers reject it until their Postgres path is wired.
    Postgres,
}

/// A declarative storage policy.
///
/// Flat and minimal today (one knob); grows new fields — a Postgres URL
/// reference, a coordination doorbell mode — as those features land. The
/// `#[serde(default)]` makes every field optional, so a bare `[store]` section
/// (or none at all) resolves to the safe, daemonless SQLite default.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct StorePolicy {
    /// Which backend to open.
    pub backend: BackendKind,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_safe_sqlite() {
        // Regression: the default must stay SQLite, so adopting a policy is a
        // no-op until a value is deliberately set (the low-risk guarantee).
        assert_eq!(StorePolicy::default().backend, BackendKind::Sqlite);
    }

    #[test]
    fn deserializes_flat_lowercase_knob() {
        let p: StorePolicy = toml::from_str(r#"backend = "postgres""#).unwrap();
        assert_eq!(p.backend, BackendKind::Postgres);

        let p: StorePolicy = toml::from_str(r#"backend = "sqlite""#).unwrap();
        assert_eq!(p.backend, BackendKind::Sqlite);
    }

    #[test]
    fn empty_config_is_the_default() {
        let p: StorePolicy = toml::from_str("").unwrap();
        assert_eq!(p.backend, BackendKind::Sqlite);
    }

    #[test]
    fn unknown_backend_is_an_error() {
        assert!(toml::from_str::<StorePolicy>(r#"backend = "mongo""#).is_err());
    }
}
