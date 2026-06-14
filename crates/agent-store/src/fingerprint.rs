//! Writer identity.
//!
//! A [`Fingerprint`] is a 32-byte BLAKE3 digest of an ed25519 public key —
//! the same derivation `agent-mesh-protocol` uses for its `Fingerprint`, so
//! the two are **wire-compatible**: the hex string a newt writes here is the
//! hex string its mesh identity announces.
//!
//! It is kept local (rather than depending on `agent-mesh-protocol`) so the
//! substrate carries no hard dependency on a specific mesh-protocol release.
//! A future `mesh` feature can add `From`/`Into` conversions once a consumer
//! needs them.

use std::fmt;

/// A 32-byte writer identity (BLAKE3 of an ed25519 public key).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint([u8; 32]);

impl Fingerprint {
    /// Wrap 32 raw bytes that are already a fingerprint.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Derive a fingerprint from an ed25519 public key (BLAKE3 of the key
    /// bytes). Deterministic and wire-compatible with the mesh.
    pub fn from_ed25519_pubkey(pubkey: &[u8]) -> Self {
        Self(*blake3::hash(pubkey).as_bytes())
    }

    /// The raw 32 bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lower-case hex (64 chars) — the canonical string form used as the
    /// `writer` column in [`crate::WriterLog`].
    pub fn to_hex(&self) -> String {
        blake3::Hash::from_bytes(self.0).to_hex().to_string()
    }

    /// Parse a 64-char lower-case hex string back into a fingerprint.
    pub fn from_hex(s: &str) -> Option<Self> {
        blake3::Hash::from_hex(s).ok().map(|h| Self(*h.as_bytes()))
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fingerprint({})", self.to_hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derivation_is_deterministic() {
        let a = Fingerprint::from_ed25519_pubkey(b"some-ed25519-pubkey-bytes");
        let b = Fingerprint::from_ed25519_pubkey(b"some-ed25519-pubkey-bytes");
        assert_eq!(a, b);
    }

    #[test]
    fn different_keys_differ() {
        let a = Fingerprint::from_ed25519_pubkey(b"key-one");
        let b = Fingerprint::from_ed25519_pubkey(b"key-two");
        assert_ne!(a, b);
    }

    #[test]
    fn hex_round_trips() {
        let fp = Fingerprint::from_ed25519_pubkey(b"round-trip-me");
        let hex = fp.to_hex();
        assert_eq!(hex.len(), 64);
        assert_eq!(Fingerprint::from_hex(&hex), Some(fp));
    }

    #[test]
    fn from_hex_rejects_garbage() {
        assert_eq!(Fingerprint::from_hex("not-hex"), None);
        assert_eq!(Fingerprint::from_hex("dead"), None); // wrong length
    }
}
