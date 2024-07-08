pub mod benchmark_configs;

use atlas_common::error::*;
use atlas_decision_log::config::DecLogConfig;
use clap::Parser;
use config::{Config, Source};
use febft_pbft_consensus::bft::config::{PBFTConfig, ProposerConfig};
use serde::Deserialize;
use std::path::PathBuf;
use std::time::Duration;

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

#[derive(Deserialize, Clone, Debug)]
pub struct FeBFTConfig {
    timeout_duration: u64,
    proposer_config: FeBFTProposerConfig,
    watermark: u32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct FeBFTProposerConfig {
    target_batch_size: u64,
    max_batch_size: u64,
    batch_timeout: u64,
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

pub fn parse_febft_conf<T>(source: T) -> Result<PBFTConfig>
where
    T: Source + Send + Sync + 'static,
{
    let mut settings = config::Config::builder().add_source(source).build()?;

    let febft_config: FeBFTConfig = settings.try_deserialize()?;

    Ok(febft_config.into())
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

impl From<FeBFTConfig> for PBFTConfig {
    fn from(value: FeBFTConfig) -> Self {
        PBFTConfig {
            timeout_dur: Duration::from_micros(value.timeout_duration),
            proposer_config: ProposerConfig {
                target_batch_size: value.proposer_config.target_batch_size,
                max_batch_size: value.proposer_config.max_batch_size,
                batch_timeout: value.proposer_config.batch_timeout,
            },
            watermark: value.watermark,
        }
    }
}
