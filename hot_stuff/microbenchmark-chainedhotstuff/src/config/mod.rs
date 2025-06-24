pub mod benchmark_configs;

use atlas_common::error::*;
use atlas_decision_log::config::DecLogConfig;
use clap::Parser;
use config::{Config, Source};
use hot_iron_oxide::config::HotIronInitConfig;
use hot_iron_oxide::crypto::QuorumInfo;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use threshold_crypto_keygen::NodeKeyPair;

#[derive(Parser, Debug)]
#[command(
    author = "Nuno Neto",
    version,
    about = "An example application utilizing Atlas's SMR replica (with monolithic state)"
)]
pub struct ReplicaArgs {
    #[arg(short, long, value_name = "DB_DIR", value_hint = clap::ValueHint::AnyPath, default_value = "./persistent_db")]
    pub db_path: PathBuf,
}

pub struct HotStuffConfig {
    pub quorum: HotIronInitConfig<QuorumInfo>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct DecisionLogConfig {
    ongoing_capacity: u32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct LogTransferConfig {
    timeout_duration: u64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ViewTransferConfig {
    timeout_duration: u64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct StateTransferConfig {
    timeout_duration: u64,
}

pub fn parse_hotstuff_config(path: PathBuf) -> Result<HotStuffConfig> {
    let NodeKeyPair {
        private_key,
        public_key,
        public_key_set,
        ..
    } = threshold_crypto_keygen::parse_key_pair(path)?;

    let quorum_info = QuorumInfo::new(private_key, public_key, public_key_set);

    Ok(HotStuffConfig {
        quorum: HotIronInitConfig { quorum_info },
    })
}

pub fn generate_hotstuff_config(f: usize) -> Vec<HotStuffConfig> {
    let vec = QuorumInfo::initialize(f);

    vec.into_iter()
        .map(|quorum| HotStuffConfig {
            quorum: HotIronInitConfig {
                quorum_info: quorum,
            },
        })
        .collect()
}

pub fn parse_dec_log_conf<T>(source: T) -> Result<DecLogConfig>
where
    T: Source + Send + Sync + 'static,
{
    let mut settings = config::Config::builder().add_source(source).build()?;

    let dec_log_config: DecisionLogConfig = settings.try_deserialize()?;

    Ok(dec_log_config.into())
}

pub fn parse_log_transfer_conf<T>(
    source: T,
) -> Result<atlas_log_transfer::config::LogTransferConfig>
where
    T: Source + Send + Sync + 'static,
{
    let mut settings = Config::builder().add_source(source).build()?;

    let lt_config: LogTransferConfig = settings.try_deserialize()?;

    Ok(lt_config.into())
}

pub fn parse_state_transfer_conf<T>(
    source: T,
) -> Result<febft_state_transfer::config::StateTransferConfig>
where
    T: Source + Send + Sync + 'static,
{
    let mut settings = Config::builder().add_source(source).build()?;

    let st_config: StateTransferConfig = settings.try_deserialize()?;

    Ok(st_config.into())
}

pub fn parse_view_transfer_conf<T>(
    source: T,
) -> Result<atlas_view_transfer::config::ViewTransferConfig>
where
    T: Source + Send + Sync + 'static,
{
    let mut settings = Config::builder().add_source(source).build()?;

    let vt_config: ViewTransferConfig = settings.try_deserialize()?;

    Ok(vt_config.into())
}

impl From<DecisionLogConfig> for DecLogConfig {
    fn from(value: DecisionLogConfig) -> Self {
        Self {
            default_ongoing_capacity: value.ongoing_capacity as usize,
        }
    }
}

impl From<LogTransferConfig> for atlas_log_transfer::config::LogTransferConfig {
    fn from(value: LogTransferConfig) -> Self {
        Self {
            timeout_duration: Duration::from_micros(value.timeout_duration),
        }
    }
}

impl From<StateTransferConfig> for febft_state_transfer::config::StateTransferConfig {
    fn from(value: StateTransferConfig) -> Self {
        Self {
            timeout_duration: Duration::from_micros(value.timeout_duration),
        }
    }
}

impl From<ViewTransferConfig> for atlas_view_transfer::config::ViewTransferConfig {
    fn from(value: ViewTransferConfig) -> Self {
        Self {
            timeout_duration: Duration::from_micros(value.timeout_duration),
        }
    }
}
