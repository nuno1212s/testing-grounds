//! Baseline executor: execution strictly after consensus commits.
//!
//! This is Atlas's standard execution path (`atlas-smr-execution`) and the reference point
//! the speculative variants are measured against. A batch reaches the executor only once the
//! decision is finalised, so the time it spends waiting on consensus -- reported as
//! `CONSENSUS_WAIT_TIME` -- is roughly a full consensus round trip. Reducing that interval is
//! the entire point of the preemptive variants.
//!
//! Requires only `Application<State>`; no CRUD interface.

use crate::common::AppNetwork;
use atlas_metrics::MetricRegistry;
use atlas_smr_execution::SingleThreadedMonExecutor;

/// Run label: logged at startup and used as the InfluxDB `extra` tag.
pub const NAME: &str = "baseline";

pub type Executor = SingleThreadedMonExecutor<AppNetwork>;

/// Metrics for the compiled-in executor.
///
/// `atlas-smr-execution` and `atlas-smr-preemptive-execution` deliberately reuse metric IDs
/// 800+, on the basis that only one of them is ever linked into a binary. Registering the
/// other crate's names here would therefore export this run's execution timings under labels
/// belonging to an executor that never ran -- corrupting the comparison silently instead of
/// failing. Each variant module registers its own, so the two cannot be mismatched.
pub fn metrics() -> Vec<MetricRegistry> {
    atlas_smr_execution::metric::metrics()
}
