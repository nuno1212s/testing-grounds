use config::Case::Upper;
use config::{Config, Environment, File, Source};
use config::FileFormat::Toml;
use getset::CopyGetters;
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug, CopyGetters)]
#[get_copy = "pub"]
pub struct BenchmarkConfig {
    pub concurrent_rqs: usize,
    pub ops_number: usize,
    pub request_sleep_millis: usize,
}

pub fn read_benchmark_config() -> atlas_common::error::Result<BenchmarkConfig> {
    read_benchmark_configs(File::new("config/benchmark_config.toml", Toml))
}

fn read_benchmark_configs<T>(source: T) -> atlas_common::error::Result<BenchmarkConfig>
where
    T: Source + Send + Sync + 'static,
{
    let benchmark_config = Config::builder()
        .add_source(source)
        .add_source(Environment::with_convert_case(Upper))
        .build()?;

    let benchmark_config: BenchmarkConfig = benchmark_config.try_deserialize()?;

    Ok(benchmark_config)
}
