#![feature(alloc_error_hook)]

use crate::common::{generate_global_log, generate_log_for_current_thread};
use crate::config::ReplicaArgs;
use crate::replica::setup_metrics;
use ::config::File;
use ::config::FileFormat::Toml;
use atlas_common::node_id::NodeId;
use atlas_common::{InitConfig, init};
use atlas_default_configs::runtime_settings::RunTimeSettings;
use clap::{Parser, Subcommand};
use std::alloc::Layout;
use std::path::Path;
use tracing::info_span;

mod client;
mod common;
mod config;
mod exec;
mod metric;
mod replica;
mod serialize;

// #[cfg(not(target_env = "msvc"))]
// use tikv_jemallocator::Jemalloc;

// #[cfg(not(target_env = "msvc"))]
// #[global_allocator]
// static GLOBAL: Jemalloc = Jemalloc;

fn main() {
    let is_client = std::env::var("CLIENT").map(|x| x == "1").unwrap_or(false);

    let n = std::env::var("N")
        .map(|x| x.parse::<usize>().unwrap())
        .unwrap_or(4);

    let runtime_config = atlas_default_configs::get_runtime_configuration()
        .expect("Failed to get runtime configurations");

    let RunTimeSettings {
        threadpool_threads,
        async_runtime_threads,
    } = runtime_config;

    let conf = InitConfig {
        //If we are the client, we want to have many threads to send stuff to replicas
        threadpool_threads,
        async_threads: async_runtime_threads,
    };

    let _guards = generate_global_log();
    
    let _guard = unsafe { init(conf).unwrap() };

    // Read all configs from the corresponding files, then create the replica config, then create the MonConfig

    if is_client {
        client::client_main();
    } else {
        let log_node_id = NodeId(0);

        let influx = atlas_default_configs::influx_db_settings::read_influx_db_config(
            File::new("config/influx_db.toml", Toml),
            Some(log_node_id),
        )
        .unwrap();

        setup_metrics(influx.into());

        let threshold_keys = Path::new("./keys/");
        
        (0..u32::try_from(n).unwrap())
            .map(|node_id| {
                std::thread::Builder::new()
                    .name(format!("replica-{}", node_id))
                    .spawn({
                        let node_id = NodeId(node_id);

                        let node_key_path = threshold_keys.join(format!("node_{}.json", node_id.0));

                        let node_key = config::parse_hotstuff_config(node_key_path)
                            .expect("Failed to parse hotstuff config");

                        let span = info_span!("NodeId", "{}", node_id.0);

                        move || {
                            let _log_guard = generate_log_for_current_thread(node_id.0);
                            replica::run_replica(node_id, node_key);
                        }
                    })
                    .expect("Failed to spawn replica thread")
            })
            .collect::<Vec<_>>()
            .into_iter()
            .for_each(|handle| {
                handle.join().expect("Failed to join replica thread");
            });
    }
}
