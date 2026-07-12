//! The client-completion oracle.
//!
//! A `CompletionLedger` records, per submitted request, whether it reached `Completed`,
//! `Failed`, or is still `Pending`. It does NOT re-implement quorum logic: it is populated
//! by wrapping the client's own submission callback, which fires only once the client has
//! matched a **2f+1** quorum of replies (F4 — `atlas-client` completes at
//! `needed_votes_count`, "in a BFT system 2f+1 by default", not f+1) or has determined the
//! request can no longer complete. So "did every request eventually get answered" becomes
//! a single programmatic assertion.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestState {
    Pending,
    /// A 2f+1 quorum of matching replies was received; `value` is the echoed value.
    Completed { value: u64 },
    /// The client reported the request can no longer complete.
    Failed { reason: String },
}

#[derive(Clone, Default)]
pub struct CompletionLedger {
    inner: Arc<Mutex<BTreeMap<u64, RequestState>>>,
}

impl CompletionLedger {
    pub fn new() -> Self {
        CompletionLedger {
            inner: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn record_pending(&self, key: u64) {
        self.inner.lock().unwrap().insert(key, RequestState::Pending);
    }

    pub fn record_completed(&self, key: u64, value: u64) {
        self.inner
            .lock()
            .unwrap()
            .insert(key, RequestState::Completed { value });
    }

    pub fn record_failed(&self, key: u64, reason: String) {
        self.inner
            .lock()
            .unwrap()
            .insert(key, RequestState::Failed { reason });
    }

    pub fn total(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn count_in(&self, want: fn(&RequestState) -> bool) -> usize {
        self.inner.lock().unwrap().values().filter(|s| want(s)).count()
    }

    pub fn pending_count(&self) -> usize {
        self.count_in(|s| matches!(s, RequestState::Pending))
    }

    pub fn completed_count(&self) -> usize {
        self.count_in(|s| matches!(s, RequestState::Completed { .. }))
    }

    pub fn failed_count(&self) -> usize {
        self.count_in(|s| matches!(s, RequestState::Failed { .. }))
    }

    /// Every recorded request has reached `Completed` (none pending, none failed).
    pub fn all_completed(&self) -> bool {
        let map = self.inner.lock().unwrap();
        !map.is_empty()
            && map
                .values()
                .all(|s| matches!(s, RequestState::Completed { .. }))
    }

    pub fn state(&self, key: u64) -> Option<RequestState> {
        self.inner.lock().unwrap().get(&key).cloned()
    }

    /// A human-readable summary for assertion failure messages.
    pub fn summary(&self) -> String {
        format!(
            "{} total: {} completed, {} pending, {} failed",
            self.total(),
            self.completed_count(),
            self.pending_count(),
            self.failed_count()
        )
    }
}
