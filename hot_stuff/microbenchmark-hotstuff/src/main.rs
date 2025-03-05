#![feature(alloc_error_hook)]

use atlas_common::{init, InitConfig};
use atlas_default_configs::runtime_settings::RunTimeSettings;
use std::alloc::Layout;
use std::path::{Path, PathBuf};
use ::config::File;
use ::config::FileFormat::Toml;
use atlas_common::node_id::NodeId;
use log::log;
use crate::common::generate_log;
use crate::replica::setup_metrics;

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

fn custom_alloc_error_hook(layout: Layout) {
    panic!("allocation error: {:?} bytes", layout.size())
}

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
        
        let threshold_keys = Path::new("./threshold_crypto_keys/");

        let _log_guard = generate_log(log_node_id.0);
        
        (0..n).map(|node_id| {
            std::thread::Builder::new()
                .name(format!("replica-{}", node_id))
                .spawn({
                    let node_id = NodeId(node_id as u32);

                    let node_key_path = threshold_keys.join(format!("node_{}.json", node_id.0));
                    
                    let node_key = config::parse_hotstuff_config(node_key_path)
                        .expect("Failed to parse hotstuff config");

                    move || replica::run_replica(node_id, node_key)
                })
                .expect("Failed to spawn replica thread")
        }).collect::<Vec<_>>().into_iter().for_each(|handle| {
            handle.join().expect("Failed to join replica thread");
        });
    }
}
