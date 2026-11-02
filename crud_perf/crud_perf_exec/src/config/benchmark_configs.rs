use config::FileFormat::Toml;
use config::{Config, Environment, File, Source};
use getset::CopyGetters;
use serde::Deserialize;
use std::time::Duration;
use tracing::info;

use crate::collision::{expected_collision_rate, key_space_for_collision_rate};

#[derive(Deserialize, Clone, Debug, CopyGetters)]
pub struct BenchmarkConfig {
    #[get_copy = "pub"]
    pub concurrent_rqs: usize,
    #[get_copy = "pub"]
    pub ops_number: usize,
    #[get_copy = "pub"]
    pub request_sleep_millis: usize,

    pub workload_type: String,
    #[get_copy = "pub"]
    pub key_space_size: usize,
    #[get_copy = "pub"]
    pub expensive_op_sleep_ms: usize,
    /// Per-operation cost in microseconds; overrides `expensive_op_sleep_ms` when non-zero.
    #[get_copy = "pub"]
    pub expensive_op_sleep_us: u64,

    /// Time-based phases: when `measure_secs > 0`, warm up for `warmup_secs`, then measure for
    /// `measure_secs`; `ops_number` is ignored.
    #[get_copy = "pub"]
    pub warmup_secs: u64,
    #[get_copy = "pub"]
    pub measure_secs: u64,

    /// Whether each request also reports two points to InfluxDB (heavy at benchmark throughput).
    #[get_copy = "pub"]
    pub client_request_metrics: bool,
    #[get_copy = "pub"]
    pub force_ordered: bool,
    pub key_distribution: String,
    #[get_copy = "pub"]
    pub zipf_constant: f64,

    /// When > 0.0, overrides key_space_size at runtime.
    /// Set to a value in (0.0, 1.0] to target that fraction of requests
    /// experiencing at least one key collision per batch.
    /// Requires expected_batch_size to be set (or defaults to concurrent_rqs).
    #[get_copy = "pub"]
    pub target_collision_rate: f64,

    /// Expected number of requests per consensus batch.
    /// Used only when target_collision_rate > 0.
    /// Defaults to concurrent_rqs when 0.
    #[get_copy = "pub"]
    pub expected_batch_size: usize,

    #[get_copy = "pub"]
    pub read_ratio: u32,
    #[get_copy = "pub"]
    pub create_ratio: u32,
    #[get_copy = "pub"]
    pub update_ratio: u32,
    #[get_copy = "pub"]
    pub delete_ratio: u32,
}

impl BenchmarkConfig {
    /// Cost of one expensive operation.
    pub fn op_cost(&self) -> Duration {
        if self.expensive_op_sleep_us > 0 {
            Duration::from_micros(self.expensive_op_sleep_us)
        } else {
            Duration::from_millis(self.expensive_op_sleep_ms as u64)
        }
    }

    /// Resolves the effective key space size.
    /// If `target_collision_rate > 0`, computes it from the birthday-problem model;
    /// otherwise returns the configured `key_space_size` directly.
    pub fn effective_key_space_size(&self) -> usize {
        if self.target_collision_rate > 0.0 {
            let batch = if self.expected_batch_size > 0 {
                self.expected_batch_size
            } else {
                self.concurrent_rqs
            };
            let ks = key_space_for_collision_rate(self.target_collision_rate, batch);
            let actual = expected_collision_rate(ks, batch);
            info!(
                "target_collision_rate={:.1}% → key_space_size={} (actual rate ≈ {:.2}%, batch_size={})",
                self.target_collision_rate * 100.0,
                ks,
                actual * 100.0,
                batch,
            );
            ks as usize
        } else {
            self.key_space_size
        }
    }
}

#[derive(Deserialize, Clone, Debug, CopyGetters)]
pub struct ClientConfig {
    #[get_copy = "pub"]
    pub clients_to_run: u16,
}

/// Environment variables as overrides (`WORKLOAD_TYPE` -> `workload_type`). No case conversion:
/// `Case::Upper` yields "UPPER CASE" with spaces, which matched no field and silently ignored
/// every variable.
fn env_source() -> Environment {
    Environment::default()
}

pub fn read_benchmark_config() -> atlas_common::error::Result<BenchmarkConfig> {
    read_benchmark_configs(File::new("config/benchmark_config.toml", Toml))
}

pub fn read_client_config() -> atlas_common::error::Result<ClientConfig> {
    read_client_config_(File::new("config/client_config.toml", Toml))
}

fn read_client_config_<T>(source: T) -> atlas_common::error::Result<ClientConfig>
where
    T: Source + Send + Sync + 'static,
{
    let client_config = Config::builder()
        .add_source(source)
        .add_source(env_source())
        .set_default("clients_to_run", 1)?
        .build()?;

    let client_config: ClientConfig = client_config.try_deserialize()?;
    Ok(client_config)
}

fn read_benchmark_configs<T>(source: T) -> atlas_common::error::Result<BenchmarkConfig>
where
    T: Source + Send + Sync + 'static,
{
    let benchmark_config = Config::builder()
        .add_source(source)
        .add_source(env_source())
        .set_default("workload_type", "uniform_cheap")?
        .set_default("key_space_size", 1_000_000i64)?
        .set_default("expensive_op_sleep_ms", 10i64)?
        .set_default("expensive_op_sleep_us", 0i64)?
        .set_default("warmup_secs", 0i64)?
        .set_default("measure_secs", 0i64)?
        .set_default("client_request_metrics", true)?
        .set_default("force_ordered", false)?
        .set_default("key_distribution", "uniform")?
        .set_default("zipf_constant", 0.1f64)?
        .set_default("target_collision_rate", 0.0f64)?
        .set_default("expected_batch_size", 0i64)?
        .set_default("read_ratio", 70i64)?
        .set_default("create_ratio", 15i64)?
        .set_default("update_ratio", 10i64)?
        .set_default("delete_ratio", 5i64)?
        .build()?;

    let benchmark_config: BenchmarkConfig = benchmark_config.try_deserialize()?;
    Ok(benchmark_config)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOML: &str = "concurrent_rqs=7\nops_number=1000\nrequest_sleep_millis=0\n";

    fn read() -> BenchmarkConfig {
        read_benchmark_configs(File::from_str(TOML, Toml)).unwrap()
    }

    /// Environment variables are process-global, so everything that touches them is one test.
    #[test]
    fn environment_variables_reach_the_benchmark_config() {
        let base = read();
        assert_eq!(base.workload_type, "uniform_cheap");
        assert_eq!(base.measure_secs(), 0);
        assert_eq!(base.concurrent_rqs(), 7);

        // SAFETY: no other test in this binary reads or writes the environment.
        unsafe {
            std::env::set_var("WORKLOAD_TYPE", "mixed_8020");
            std::env::set_var("EXPENSIVE_OP_SLEEP_US", "250");
            std::env::set_var("MEASURE_SECS", "15");
            std::env::set_var("WARMUP_SECS", "5");
            std::env::set_var("FORCE_ORDERED", "true");
            std::env::set_var("CLIENT_REQUEST_METRICS", "0");
            std::env::set_var("KEY_SPACE_SIZE", "4242");
            std::env::set_var("CONCURRENT_RQS", "3");
        }

        let cfg = read();
        assert_eq!(cfg.workload_type, "mixed_8020", "WORKLOAD_TYPE ignored");
        assert_eq!(cfg.expensive_op_sleep_us(), 250, "EXPENSIVE_OP_SLEEP_US ignored");
        assert_eq!(cfg.measure_secs(), 15, "MEASURE_SECS ignored");
        assert_eq!(cfg.warmup_secs(), 5, "WARMUP_SECS ignored");
        assert!(cfg.force_ordered(), "FORCE_ORDERED ignored");
        assert!(!cfg.client_request_metrics(), "CLIENT_REQUEST_METRICS ignored");
        assert_eq!(cfg.key_space_size(), 4242, "KEY_SPACE_SIZE ignored");
        assert_eq!(cfg.concurrent_rqs(), 3, "an environment value must outrank the file");
        assert_eq!(cfg.op_cost(), Duration::from_micros(250));
    }
}
