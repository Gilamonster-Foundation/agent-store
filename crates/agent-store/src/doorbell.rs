//! The commit doorbell — the seam for local multi-agent coordination.
//!
//! When an agent commits a turn, co-located agents should wake and read it
//! instead of polling the file. The substrate does **not** know about the
//! mesh: it only emits a [`CommitEvent`]. A consumer (newt's session loop,
//! modulex's MCP server) subscribes and bridges the event onto agent-mesh —
//! publishing the causal pointer `(writer, seq)` on a per-stream topic. A peer
//! that misses the doorbell still catches up by reading entries past its
//! last-seen seq on its next load.
//!
//! The payload is a **causal pointer, never a timestamp** — it composes with
//! the mesh's per-peer sequence tracking and honors "wall-clock is a claim,
//! never a coordination primitive."

use std::sync::{Arc, Mutex};

/// What a commit announces: where it landed and its content hash. Just enough
/// for a peer to fetch the entry — never the payload itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitEvent {
    pub stream: String,
    pub writer: String,
    pub seq: u64,
    pub content_hash: [u8; 32],
}

type Subscriber = Box<dyn Fn(&CommitEvent) + Send + Sync>;

/// A fan-out of commit subscribers. Cloneable and shareable; clones observe
/// the same subscriber set.
#[derive(Clone, Default)]
pub struct Doorbell {
    subscribers: Arc<Mutex<Vec<Subscriber>>>,
}

impl Doorbell {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a callback invoked for every subsequent [`Doorbell::ring`].
    pub fn subscribe<F>(&self, f: F)
    where
        F: Fn(&CommitEvent) + Send + Sync + 'static,
    {
        self.subscribers
            .lock()
            .expect("doorbell mutex poisoned")
            .push(Box::new(f));
    }

    /// Notify every subscriber of a commit. Consumers call this immediately
    /// after a successful [`crate::WriterLog::append`].
    pub fn ring(&self, event: &CommitEvent) {
        for sub in self
            .subscribers
            .lock()
            .expect("doorbell mutex poisoned")
            .iter()
        {
            sub(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivers_events_to_subscribers() {
        let bell = Doorbell::new();
        let seen: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_clone = Arc::clone(&seen);
        bell.subscribe(move |e| seen_clone.lock().unwrap().push(e.seq));

        bell.ring(&CommitEvent {
            stream: "conv:x".into(),
            writer: "alice".into(),
            seq: 1,
            content_hash: [0u8; 32],
        });
        bell.ring(&CommitEvent {
            stream: "conv:x".into(),
            writer: "alice".into(),
            seq: 2,
            content_hash: [0u8; 32],
        });

        assert_eq!(*seen.lock().unwrap(), vec![1, 2]);
    }

    #[test]
    fn multiple_subscribers_all_fire() {
        let bell = Doorbell::new();
        let count = Arc::new(Mutex::new(0u32));
        for _ in 0..3 {
            let c = Arc::clone(&count);
            bell.subscribe(move |_| *c.lock().unwrap() += 1);
        }
        bell.ring(&CommitEvent {
            stream: "s".into(),
            writer: "w".into(),
            seq: 1,
            content_hash: [0u8; 32],
        });
        assert_eq!(*count.lock().unwrap(), 3);
    }
}
