use atlas_metrics::MetricRegistry;
use atlas_metrics::metrics::MetricKind;

pub const OPERATIONS_EXECUTED_PER_SECOND: &str = "OPERATIONS_EXECUTED_PER_SECOND";
pub const OPERATIONS_EXECUTED_PER_SECOND_ID: usize = 0;

pub const CLIENT_LATENCY: &str = "CLIENT_RQ_LATENCY";
pub const CLIENT_LATENCY_ID: usize = 1;

pub fn metrics() -> Vec<MetricRegistry> {
    
    vec![
        (
            OPERATIONS_EXECUTED_PER_SECOND_ID,
            OPERATIONS_EXECUTED_PER_SECOND.to_string(),
            MetricKind::Counter
        ).into(),
        (
            CLIENT_LATENCY_ID,
            CLIENT_LATENCY.to_string(),
            MetricKind::Duration
        ).into(),
    ]
    
}