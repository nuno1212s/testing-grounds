use config::Case::Upper;
use config::FileFormat::Toml;
use config::{Config, Environment, File, Source};
use getset::CopyGetters;
use serde::Deserialize;
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
        .add_source(Environment::with_convert_case(Upper))
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
        .add_source(Environment::with_convert_case(Upper))
        .set_default("workload_type", "uniform_cheap")?
        .set_default("key_space_size", 1_000_000i64)?
        .set_default("expensive_op_sleep_ms", 10i64)?
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
