use std::path::PathBuf;

use anyhow::anyhow;
use clap::Parser;
use config::File;
use config::FileFormat::Toml;
use tracing::{error, info};

use atlas_comm_mio::config::MIOConfig;
use atlas_common::async_runtime;
use atlas_common::ordering::SeqNo;
use atlas_decision_log::config::DecLogConfig;
use atlas_default_configs::{get_network_configurations, get_reconfig_config};
use atlas_log_transfer::config::LogTransferConfig;
use atlas_metrics::{InfluxDBArgs, MetricLevel, with_metric_level, with_metrics};
use atlas_reconfiguration::config::ReconfigurableNetworkConfig;
use atlas_smr_replica::server::monolithic_server::MonReplica;
use atlas_view_transfer::config::ViewTransferConfig;
use febft_pbft_consensus::bft::config::PBFTConfig;
use febft_state_transfer::config::StateTransferConfig;

use crate::common::{generate_log, MonConfig, ReplicaConf, SMRReplica};
use crate::config::ReplicaArgs;
use crate::exec::Microbenchmark;

pub fn init_replica_config(
    reconf: ReconfigurableNetworkConfig,
    network: MIOConfig,
    order_protocol_config: PBFTConfig,
    log_transfer_config: LogTransferConfig,
    dec_log_config: DecLogConfig,
    view_transfer_config: ViewTransferConfig,
    db_path: PathBuf,
) -> atlas_common::error::Result<ReplicaConf> {
    let db_path = db_path.into_os_string().into_string();

    let db_path = match db_path {
        Ok(db) => db,
        Err(_) => {
            return Err(anyhow!("Failed to parse persistent log folder"));
        }
    };

    let conf = ReplicaConf {
        node: network,
        next_consensus_seq: SeqNo::ZERO,
        op_config: order_protocol_config,
        dl_config: dec_log_config,
        lt_config: log_transfer_config,
        db_path,
        pl_config: (),
        reconfig_node: reconf,
        vt_config: view_transfer_config,
        p: Default::default(),
        preprocessor_threads: 3
    };

    Ok(conf)
}

pub fn init_mon_replica_conf(
    replica_conf: ReplicaConf,
    state_transfer_config: StateTransferConfig,
    service: Microbenchmark,
) -> atlas_common::error::Result<MonConfig> {
    Ok(MonConfig {
        service,
        replica_config: replica_conf,
        st_config: state_transfer_config,
    })
}



pub(super) fn setup_metrics(influx: InfluxDBArgs) {
    atlas_metrics::initialize_metrics(
        vec![
            with_metrics(febft_pbft_consensus::bft::metric::metrics()),
            with_metrics(atlas_core::metric::metrics()),
            with_metrics(atlas_communication::metric::metrics()),
            with_metrics(atlas_smr_replica::metric::metrics()),
            with_metrics(atlas_smr_core::metric::metrics()),
            with_metrics(atlas_smr_execution::metric::metrics()),
            with_metrics(atlas_log_transfer::metrics::metrics()),
            with_metrics(febft_state_transfer::metrics::metrics()),
            with_metrics(atlas_view_transfer::metrics::metrics()),
            with_metrics(atlas_comm_mio::metrics::metrics()),
            with_metric_level(MetricLevel::Trace),
        ],
        influx,
    );
}

pub(super) fn run_replica() {
    let replica_args = ReplicaArgs::parse();

    let reconfiguration_cfg = get_reconfig_config().unwrap();
    
    let node_id = reconfiguration_cfg.node_id;
    
    let influx = atlas_default_configs::influx_db_settings::read_influx_db_config(File::new("config/influx_db.toml", Toml), Some(node_id)).unwrap();

    setup_metrics(influx.into());
    
    let _log_guard = generate_log(node_id.0);
    
    info!("Initializing node with config {:?}", reconfiguration_cfg);

    let (network_cfg, pool_config) = get_network_configurations(node_id).unwrap();

    info!("Parsing febft config");
    // Read all configs from the corresponding files, then create the replica config, then create the MonConfig
    let config = crate::config::parse_febft_conf(File::new("config/febft.toml", Toml)).unwrap();

    info!("Parsing decision log config");
    let dec_log_config =
        crate::config::parse_dec_log_conf(File::new("config/dec_log.toml", Toml)).unwrap();

    info!("Parsing log transfer config");
    let log_transfer =
        crate::config::parse_log_transfer_conf(File::new("config/log_transfer.toml", Toml))
            .unwrap();

    info!("Parsing state transfer config");
    let state_transfer =
        crate::config::parse_state_transfer_conf(File::new("config/state_transfer.toml", Toml))
            .unwrap();

    info!("Parsing view transfer config");
    let view_transfer =
        crate::config::parse_view_transfer_conf(File::new("config/view_transfer.toml", Toml))
            .unwrap();

    info!("Setting up replica configuration");

    let replica_config = init_replica_config(
        reconfiguration_cfg,
        network_cfg,
        config,
        log_transfer,
        dec_log_config,
        view_transfer,
        replica_args.db_path,
    )
    .unwrap();

    let mon_config =
        init_mon_replica_conf(replica_config, state_transfer, Microbenchmark::new(node_id))
            .unwrap();

    info!("Bootstrapping replica");

    let mut replica: SMRReplica =
        async_runtime::block_on(MonReplica::bootstrap(mon_config)).unwrap();

    info!("Running replica");

    loop {
        if let Err(err) = replica.run(None) {
            error!("Error while executing replica {}", err);
        }
    }
}
