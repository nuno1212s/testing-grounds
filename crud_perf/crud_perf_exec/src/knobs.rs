//! Run-time knobs read from the environment by every node. Malformed values panic rather than
//! fall back, so a run cannot silently use the wrong cost model or state.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use tracing::info;

/// Length of every workload-written and preloaded value.
pub const VALUE_LEN: usize = 64;

/// How a request's `time_delay` is spent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessingMode {
    /// `thread::sleep`: costs latency, not CPU (I/O-bound work).
    Sleep,
    /// Busy-wait: holds a core (CPU-bound work); exact at sub-millisecond durations.
    Spin,
}

pub fn processing_mode() -> ProcessingMode {
    static MODE: OnceLock<ProcessingMode> = OnceLock::new();

    *MODE.get_or_init(|| match std::env::var("PROCESSING_MODE").as_deref() {
        Err(_) | Ok("") | Ok("sleep") => ProcessingMode::Sleep,
        Ok("spin") => ProcessingMode::Spin,
        Ok(other) => panic!("PROCESSING_MODE must be 'sleep' or 'spin', got {other:?}"),
    })
}

/// Spend `duration` on simulated application work.
pub fn simulate_work(duration: Duration) {
    if duration.is_zero() {
        return;
    }

    match processing_mode() {
        ProcessingMode::Sleep => std::thread::sleep(duration),
        ProcessingMode::Spin => {
            let deadline = Instant::now() + duration;
            while Instant::now() < deadline {
                std::hint::spin_loop();
            }
        }
    }
}

/// Keys `0..n` present in the state before the first request, so state size does not depend
/// on warm-up speed. Identical on every replica.
pub fn preload_keys() -> u64 {
    static KEYS: OnceLock<u64> = OnceLock::new();

    *KEYS.get_or_init(|| match std::env::var("PRELOAD_KEYS").as_deref() {
        Err(_) | Ok("") => 0,
        Ok(v) => v
            .parse()
            .unwrap_or_else(|_| panic!("PRELOAD_KEYS must be a non-negative integer, got {v:?}")),
    })
}

/// Log the effective values once; also surfaces a malformed one at startup.
pub fn announce(role: &str) {
    info!(
        role,
        processing_mode = ?processing_mode(),
        preload_keys = preload_keys(),
        "Effective run-time knobs"
    );
}
