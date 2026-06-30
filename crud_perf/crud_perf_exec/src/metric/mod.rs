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
    ]
}
