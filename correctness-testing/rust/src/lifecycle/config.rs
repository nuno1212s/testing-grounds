//! Scenario timing knobs, shared across protocols. The actual per-protocol config
//! structs are built in `backend.rs`.

use std::time::Duration;

/// Timing knobs for a scenario. Defaults are deliberately snappy so tests are fast, but
/// long enough that a fault-free run never spuriously triggers a view change.
#[derive(Clone, Debug)]
pub struct TimingConfig {
    pub consensus_timeout: Duration,
    pub log_transfer_timeout: Duration,
    pub view_transfer_timeout: Duration,
    pub state_transfer_timeout: Duration,
    pub batch_timeout_micros: u64,
    pub target_batch_size: u64,
    pub max_batch_size: u64,
}

impl Default for TimingConfig {
    fn default() -> Self {
        TimingConfig {
            consensus_timeout: Duration::from_secs(2),
            log_transfer_timeout: Duration::from_secs(2),
            view_transfer_timeout: Duration::from_secs(2),
            state_transfer_timeout: Duration::from_secs(2),
            // Propose promptly when a request arrives (target=1), but keep the *empty*
            // keep-alive proposals infrequent (50ms) so we don't flood the log/consensus
            // with ~1000 empty instances/sec under an idle cluster.
            batch_timeout_micros: 50_000,
            target_batch_size: 1,
            max_batch_size: 128,
        }
    }
}
