use atlas_metrics::metrics::MetricKind;
use atlas_metrics::{MetricLevel, MetricRegistry, MetricRegistryInfo};

/// crud_perf occupies the 85X metric ID sub-space.
/// IDs 800-849 are used by atlas_smr_preemptive_execution and related crates.

pub const CRUD_CLIENT_LATENCY: &str = "CRUD_CLIENT_LATENCY";
pub const CRUD_CLIENT_LATENCY_ID: usize = 850;

pub const CRUD_CLIENT_OPS_DONE: &str = "CRUD_CLIENT_OPS_DONE";
pub const CRUD_CLIENT_OPS_DONE_ID: usize = 851;

pub const CRUD_BATCH_EXEC_TIME: &str = "CRUD_BATCH_EXEC_TIME";
pub const CRUD_BATCH_EXEC_TIME_ID: usize = 852;

pub const CRUD_OP_EXEC_TIME: &str = "CRUD_OP_EXEC_TIME";
pub const CRUD_OP_EXEC_TIME_ID: usize = 853;

pub const CRUD_OPS_PER_BATCH: &str = "CRUD_OPS_PER_BATCH";
pub const CRUD_OPS_PER_BATCH_ID: usize = 854;

// ---------------------------------------------------------------------------
// Separating the speculative application path from the confirmed one (855-856)
// ---------------------------------------------------------------------------
//
// `CRUD_OP_EXEC_TIME` used to be recorded by all three of `Application::update`,
// `Application::unordered_execution` and `CRUDApplication::speculatively_execute`, which
// made it useless for the one comparison this benchmark exists to make: a `dual_state` run
// executes every operation twice (once speculatively, once again on confirmation) and a
// `baseline` run executes it once, yet both reported a single indistinguishable average.
// One name per path, so each can be read on its own and the two can be added up.

/// Per-operation application time on the **speculative** path
/// (`CRUDApplication::speculatively_execute`, against the delta/cache proxy).
///
/// Empty for `baseline`, which has no speculative path. Compare against
/// `CRUD_OP_EXEC_TIME`: a speculative operation is doing the same application work
/// through a `CRUDState` proxy instead of the real state, so a large gap is the
/// proxy's overhead, not the application's.
pub const CRUD_SPEC_OP_EXEC_TIME: &str = "CRUD_SPEC_OP_EXEC_TIME";
pub const CRUD_SPEC_OP_EXEC_TIME_ID: usize = 855;

/// Per-operation application time for unordered (read-only) execution, which never
/// reaches consensus and so is unaffected by the executor variant. Split out so it
/// stops diluting `CRUD_OP_EXEC_TIME`, which is meant to be the ordered path only.
pub const CRUD_UNORDERED_OP_EXEC_TIME: &str = "CRUD_UNORDERED_OP_EXEC_TIME";
pub const CRUD_UNORDERED_OP_EXEC_TIME_ID: usize = 856;

// ---------------------------------------------------------------------------
// Client latency by operation kind (857-859)
// ---------------------------------------------------------------------------
//
// `CRUD_CLIENT_LATENCY` mixes the whole workload into one distribution, and the default
// mix is 70/15/10/5 -- so it is dominated by reads and the effect of speculation on
// writes is averaged away inside it. Speculation does different work for each kind
// (a read is served from the accumulated cache, a write accumulates a delta, and the
// scalable executor's collision detection is driven by writes alone), so each kind
// needs its own distribution.
//
// Exactly one of the three is recorded per request, alongside the existing aggregate,
// so this costs one extra InfluxDB point per request rather than three.
//
// `CorrelationDurationTracker` writes one point per request instead of a per-second
// average, which is what makes real percentiles possible -- the rest of the bench has
// already been collapsed to a mean by the time it reaches InfluxDB.

/// End-to-end client latency for `Read` requests.
pub const CRUD_LATENCY_READ: &str = "CRUD_LATENCY_READ";
pub const CRUD_LATENCY_READ_ID: usize = 857;

/// End-to-end client latency for `Create` and `Update` requests -- the two that
/// `handle_write`/`CRUDState::update` treat identically.
pub const CRUD_LATENCY_WRITE: &str = "CRUD_LATENCY_WRITE";
pub const CRUD_LATENCY_WRITE_ID: usize = 858;

/// End-to-end client latency for `Delete` requests.
pub const CRUD_LATENCY_DELETE: &str = "CRUD_LATENCY_DELETE";
pub const CRUD_LATENCY_DELETE_ID: usize = 859;

pub fn metrics() -> Vec<MetricRegistry> {
    vec![
        MetricRegistryInfo::from((
            CRUD_CLIENT_LATENCY_ID,
            CRUD_CLIENT_LATENCY.to_string(),
            MetricKind::CorrelationDurationTracker,
            MetricLevel::Info,
        )),
        MetricRegistryInfo::from((
            CRUD_CLIENT_OPS_DONE_ID,
            CRUD_CLIENT_OPS_DONE.to_string(),
            MetricKind::Counter,
            MetricLevel::Info,
        )),
        MetricRegistryInfo::from((
            CRUD_BATCH_EXEC_TIME_ID,
            CRUD_BATCH_EXEC_TIME.to_string(),
            MetricKind::Duration,
            MetricLevel::Info,
        )),
        MetricRegistryInfo::from((
            CRUD_OP_EXEC_TIME_ID,
            CRUD_OP_EXEC_TIME.to_string(),
            MetricKind::Duration,
            MetricLevel::Info,
        )),
        MetricRegistryInfo::from((
            CRUD_OPS_PER_BATCH_ID,
            CRUD_OPS_PER_BATCH.to_string(),
            MetricKind::Count,
            MetricLevel::Debug,
        )),
        MetricRegistryInfo::from((
            CRUD_SPEC_OP_EXEC_TIME_ID,
            CRUD_SPEC_OP_EXEC_TIME.to_string(),
            MetricKind::Duration,
            MetricLevel::Info,
        )),
        MetricRegistryInfo::from((
            CRUD_UNORDERED_OP_EXEC_TIME_ID,
            CRUD_UNORDERED_OP_EXEC_TIME.to_string(),
            MetricKind::Duration,
            MetricLevel::Info,
        )),
        MetricRegistryInfo::from((
            CRUD_LATENCY_READ_ID,
            CRUD_LATENCY_READ.to_string(),
            MetricKind::CorrelationDurationTracker,
            MetricLevel::Info,
        )),
        MetricRegistryInfo::from((
            CRUD_LATENCY_WRITE_ID,
            CRUD_LATENCY_WRITE.to_string(),
            MetricKind::CorrelationDurationTracker,
            MetricLevel::Info,
        )),
        MetricRegistryInfo::from((
            CRUD_LATENCY_DELETE_ID,
            CRUD_LATENCY_DELETE.to_string(),
            MetricKind::CorrelationDurationTracker,
            MetricLevel::Info,
        )),
    ]
}
