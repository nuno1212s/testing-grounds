use atlas_metrics::metrics::MetricKind;
use atlas_metrics::MetricRegistry;

pub(crate) const RQS_MADE_ID: usize = 1300;
pub(crate) const RQS_MADE: &str = "TEST_RQS_MADE";

pub(crate) const RQS_MADE_IND_ID: usize = 1301;
pub(crate) const RQS_MADE_IND: &str = "TEST_RQS_MADE_IND";

pub fn metrics() -> Vec<MetricRegistry> {
    vec![
        (
            RQS_MADE_ID,
            RQS_MADE.to_string(),
            MetricKind::Counter,
        )
            .into(),
        (
            RQS_MADE_IND_ID,
            RQS_MADE_IND.to_string(),
            MetricKind::CounterCorrelation,
        )
            .into(),
    ]
}
