use config::FileFormat::Toml;
use config::{Config, Environment, File};
use getset::{CopyGetters, Getters};
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, Getters)]
pub struct ClusterConfig {
    #[get = "pub"]
    machine_list: Vec<MachineConfig>,
}

fn default_base_port() -> i32 {
    10000
}

#[derive(Deserialize, Debug, Clone, Getters, CopyGetters)]
pub struct MachineConfig {
    #[get = "pub"]
    name: String,
    #[get = "pub"]
    machine_ip: String,
    #[get_copy = "pub"]
    #[serde(default = "default_base_port")]
    base_port: i32,
    #[get = "pub"]
    #[serde(default)]
    restrictions: Option<MachineRestrictions>,
}

#[derive(Deserialize, Debug, Clone, CopyGetters)]
pub struct MachineRestrictions {
    #[get_copy = "pub"]
    max_replicas: usize,
    #[get_copy = "pub"]
    max_clients: usize,
}

pub fn get_cluster_config() -> anyhow::Result<ClusterConfig> {
    let config = Config::builder()
        .add_source(File::new("config/cluster.toml", Toml))
        .add_source(Environment::with_prefix("CLUSTER"))
        .build()?;

    Ok(config.try_deserialize()?)
}
