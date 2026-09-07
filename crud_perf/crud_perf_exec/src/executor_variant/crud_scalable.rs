//! Parallel cache-based speculative executor.
//!
//! Same confirm/backtrack model as [`super::crud_single`] -- deltas applied on confirmation,
//! discarded on backtrack -- but the requests *within* a batch execute in parallel on a rayon
//! pool. Each operation runs through an execution unit that records its data accesses;
//! operations that touch a key another operation wrote or deleted are flagged as colliding
//! and re-executed sequentially, so the result still matches a serial execution.
//!
//! Whether this beats [`super::crud_single`] depends entirely on the workload's key
//! contention: with a large key space collisions are rare and the batch parallelises, while
//! under a hot-key or small-keyspace workload most operations collide and it degenerates
//! towards sequential execution plus the cost of having tried. `SCALABLE_COLLISION_RATE` is
//! the metric that tells those two regimes apart, so sweep `KEY_SPACE_SIZE` and
//! `KEY_DISTRIBUTION` when benchmarking this variant.

use atlas_metrics::MetricRegistry;
use atlas_smr_preemptive_execution::ScalableCRUDMonolithicPreemptiveExecutor;

/// Run label: logged at startup and used as the InfluxDB `extra` tag.
pub const NAME: &str = "crud_scalable";

pub type Executor = ScalableCRUDMonolithicPreemptiveExecutor;

/// Metrics for the compiled-in executor.
///
/// See the note in [`super::baseline::metrics`]: the baseline and preemptive crates share
/// metric IDs 800+, so registering the wrong crate would mislabel this run's data rather
/// than fail. Relevant here: `SCALABLE_PREEMPTIVE_EXECUTION_TIME`, `SCALABLE_COLLISION_RATE`,
/// `SCALABLE_COLLISION_COUNT`, plus the shared `CACHE_*` state-machine metrics.
///
/// `OPERATIONS_EXECUTED_PER_SECOND` is the exception to the "different names" rule: the
/// preemptive crate exports it under the same name as the baseline, counted at
/// confirmation, so throughput can be compared against `baseline` panel-for-panel.
pub fn metrics() -> Vec<MetricRegistry> {
    atlas_smr_preemptive_execution::metric::metrics()
}
