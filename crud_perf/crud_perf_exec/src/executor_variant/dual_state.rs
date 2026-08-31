//! Dual-state speculative executor.
//!
//! Keeps two complete copies of the application state and two worker threads. The preemptive
//! worker executes a batch against the speculative state as soon as the ordering protocol
//! proposes it, buffering the replies; on confirmation the batch is handed to the confirmed
//! worker, which **re-executes** it against the authoritative state and releases the replies.
//!
//! The trade-off versus the cache-based variants: it needs only `Application<State>` (no CRUD
//! interface), but pays two full state copies in memory, a full re-execution per confirmed
//! batch, and a state clone on every backtrack.
//!
//! Known gap: `single_thread_double_state/preemptive_worker` still has a `todo!()` on the
//! path that handles a confirmed-worker message while in state-transfer mode. It is off the
//! steady-state path, but a run that triggers state transfer (replica crash-and-rejoin, log
//! watermark overflow, partition-induced CST) can panic. Keep steady-state latency runs free
//! of forced state transfer.

use atlas_metrics::MetricRegistry;
use atlas_smr_preemptive_execution::MonolithicPreemptiveExecutor;

/// Run label: logged at startup and used as the InfluxDB `extra` tag.
pub const NAME: &str = "dual_state";

pub type Executor = MonolithicPreemptiveExecutor;

/// Metrics for the compiled-in executor.
///
/// See the note in [`super::baseline::metrics`]: the baseline and preemptive crates share
/// metric IDs 800+, so registering the wrong crate would mislabel this run's data rather
/// than fail. Relevant here: `DS_PREEMPTIVE_EXECUTION_TIME`, `DS_BACKTRACK_COUNT`,
/// `DS_SPECULATION_TO_CONFIRM_LATENCY`, `CONFIRM_EXECUTION_TIME`.
pub fn metrics() -> Vec<MetricRegistry> {
    atlas_smr_preemptive_execution::metric::metrics()
}
