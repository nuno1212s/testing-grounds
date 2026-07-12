//! A pool of concurrent in-flight client requests, wired to a `CompletionLedger`.
//!
//! Wraps a real `atlas_client` `ConcurrentClient` and submits ordered requests
//! non-blocking via `update_imm_callback`, so faults can be injected while requests are
//! in flight. Each request is recorded `Pending` at submit and flipped to
//! `Completed`/`Failed` by the client's own reply callback (2f+1 quorum, F4).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use atlas_client::client::ordered_client::Ordered;
use atlas_client::concurrent_client::ConcurrentClient;

use crate::apps::kv_echo::EchoRequest;
use crate::composition::{AppData, ClientNetwork, ReconfProtocol, SMRClient};
use crate::oracle::completion_ledger::CompletionLedger;

type ConcClient = ConcurrentClient<ReconfProtocol, AppData, ClientNetwork>;

pub struct ClientPool {
    client: ConcClient,
    ledger: CompletionLedger,
    next_key: AtomicU64,
}

impl ClientPool {
    /// Build a pool over an already-bootstrapped client, allowing up to `concurrency`
    /// in-flight requests.
    pub fn from_client(client: SMRClient, concurrency: usize) -> Result<Self> {
        let client = ConcurrentClient::from_client(client, concurrency)
            .map_err(|e| anyhow!("ConcurrentClient::from_client: {e}"))?;
        Ok(ClientPool {
            client,
            ledger: CompletionLedger::new(),
            next_key: AtomicU64::new(0),
        })
    }

    pub fn ledger(&self) -> &CompletionLedger {
        &self.ledger
    }

    /// Submit one ordered request without blocking; returns its ledger key. The reply
    /// callback records completion (2f+1) or failure.
    pub fn submit_ordered(&self, key: u64, value: u64) -> Result<u64> {
        let req_id = self.next_key.fetch_add(1, Ordering::Relaxed);
        self.ledger.record_pending(req_id);

        let ledger = self.ledger.clone();
        let callback: Arc<dyn Fn(atlas_common::error::Result<crate::apps::kv_echo::EchoReply>) + Send + Sync> =
            Arc::new(move |reply| match reply {
                Ok(r) => ledger.record_completed(req_id, r.value),
                Err(e) => ledger.record_failed(req_id, format!("{e}")),
            });

        self.client
            .update_imm_callback::<Ordered>(EchoRequest::new(key, value), callback)
            .map_err(|e| anyhow!("submit_ordered: {e}"))?;

        Ok(req_id)
    }

    /// Submit `n` ordered requests (keys/values derived from a base) concurrently.
    pub fn submit_n(&self, n: u64, base: u64) -> Result<()> {
        for i in 0..n {
            self.submit_ordered(base + i, base + i)?;
        }
        Ok(())
    }

    /// Halt assertion (M5): after waiting `grace`, every key in `keys` must still be
    /// `Pending` — i.e. those requests did NOT complete (liveness is correctly lost when
    /// the surviving quorum is below 2f+1). Fails if any completed or failed.
    pub fn assert_pending(&self, keys: &[u64], grace: Duration) -> Result<()> {
        std::thread::sleep(grace);
        for &k in keys {
            match self.ledger.state(k) {
                Some(crate::oracle::completion_ledger::RequestState::Pending) => {}
                other => {
                    return Err(anyhow!(
                        "request {k} should have halted (stayed Pending with no reachable \
                         quorum) but was {other:?}"
                    ));
                }
            }
        }
        Ok(())
    }

    /// Block until every submitted request has completed, or `deadline` elapses.
    /// On timeout, returns an error with the ledger summary.
    pub fn assert_all_completed(&self, deadline: Duration) -> Result<()> {
        let until = Instant::now() + deadline;
        while Instant::now() < until {
            if self.ledger.all_completed() {
                return Ok(());
            }
            if self.ledger.failed_count() > 0 {
                return Err(anyhow!(
                    "a request failed before the deadline: {}",
                    self.ledger.summary()
                ));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Err(anyhow!(
            "not all requests completed within {deadline:?}: {}",
            self.ledger.summary()
        ))
    }
}
