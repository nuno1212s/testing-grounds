//! Single-threaded cache-based speculative executor.
//!
//! One worker thread and one authoritative state. Each speculative batch executes through a
//! caching proxy: writes accumulate into a per-batch delta and the real state is never
//! touched. On confirmation the pre-computed delta is applied directly -- no re-execution.
//! On backtrack the pending deltas are discarded and the accumulated cache rebuilt -- no
//! state clone.
//!
//! Cheaper than [`super::dual_state`] on both memory and confirmation cost, at the price of
//! requiring the application to implement `CRUDState` + `CRUDApplication` (see
//! `serialize::State` and `exec::Microbenchmark`).
//!
//! Replies are computed speculatively but only released on confirmation, so clients never
//! observe a speculative result.

use atlas_metrics::MetricRegistry;
use atlas_smr_preemptive_execution::CRUDMonolithicPreemptiveExecutor;

/// Run label: logged at startup and used as the InfluxDB `extra` tag.
pub const NAME: &str = "crud_single";

pub type Executor = CRUDMonolithicPreemptiveExecutor;

/// Metrics for the compiled-in executor.
///
/// See the note in [`super::baseline::metrics`]: the baseline and preemptive crates share
/// metric IDs 800+, so registering the wrong crate would mislabel this run's data rather
/// than fail. Relevant here: `CACHE_PREEMPTIVE_EXECUTION_TIME`, `CACHE_BACKTRACK_COUNT`,
/// `CACHE_CONFIRM_APPLICATION_TIME`, `CACHE_PENDING_QUEUE_SIZE`, `CACHE_REBUILD_TIME`.
///
/// `OPERATIONS_EXECUTED_PER_SECOND` is the exception to the "different names" rule: the
/// preemptive crate exports it under the same name as the baseline, counted at
/// confirmation, so throughput can be compared against `baseline` panel-for-panel.
pub fn metrics() -> Vec<MetricRegistry> {
    atlas_smr_preemptive_execution::metric::metrics()
}
