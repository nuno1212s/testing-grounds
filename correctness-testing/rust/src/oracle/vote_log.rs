//! A record of every vote each node cast, observed at the byte gate.
//!
//! Populated by a protocol adapter's gate observer (which recognizes vote messages by
//! content). The safety question "did any node equivocate?" becomes: is there a
//! `(node, seq, phase)` for which we saw two *different* vote digests? This is
//! meaningful only with DISTINCT per-node keys (F6) — otherwise signatures/identities
//! would be indistinguishable. Used by the amnesia scenario and (later) M5 halt-not-fork.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use atlas_common::node_id::NodeId;

/// Key: (voter, consensus sequence, phase name).
type VoteKey = (NodeId, i64, &'static str);

#[derive(Clone, Default)]
pub struct VoteLog {
    inner: Arc<Mutex<BTreeMap<VoteKey, BTreeSet<String>>>>,
}

impl VoteLog {
    pub fn new() -> Self {
        VoteLog {
            inner: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// Record that `from` cast a vote for `(seq, phase)` over batch `digest`.
    pub fn record(&self, from: NodeId, seq: i64, phase: &'static str, digest: String) {
        self.inner
            .lock()
            .unwrap()
            .entry((from, seq, phase))
            .or_default()
            .insert(digest);
    }

    /// Total distinct votes observed (across all nodes/seqs/phases/digests).
    pub fn total_votes(&self) -> usize {
        self.inner.lock().unwrap().values().map(|s| s.len()).sum()
    }

    /// Every `(node, seq, phase)` for which two or more distinct digests were seen —
    /// i.e. an equivocation.
    pub fn equivocations(&self) -> Vec<(NodeId, i64, &'static str, Vec<String>)> {
        self.inner
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, digests)| digests.len() > 1)
            .map(|((node, seq, phase), digests)| {
                (*node, *seq, *phase, digests.iter().cloned().collect())
            })
            .collect()
    }

    pub fn has_equivocation(&self) -> bool {
        self.inner
            .lock()
            .unwrap()
            .values()
            .any(|digests| digests.len() > 1)
    }
}
