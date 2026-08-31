#![feature(alloc_error_hook)]

use std::alloc::Layout;

use atlas_common::{InitConfig, init};
use atlas_default_configs::runtime_settings::RunTimeSettings;

mod client;
mod collision;
mod common;
mod config;
mod exec;
mod executor_variant;
mod metric;
mod replica;
mod serialize;
mod workload;

fn custom_alloc_error_hook(layout: Layout) {
    panic!("allocation error: {:?} bytes", layout.size())
}

fn main() {
    // Collision calc mode: no Atlas runtime needed, runs before any config loading.
    let calc_collision = std::env::var("COLLISION_CALC")
        .map(|x| x == "1")
        .unwrap_or(false);

    if calc_collision {
        let batch_size: usize = std::env::var("BATCH_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(200);
        let target_rate: Option<f64> = std::env::var("TARGET_RATE")
            .ok()
            .and_then(|s| s.parse().ok());
        let key_space: Option<u64> = std::env::var("KEY_SPACE").ok().and_then(|s| s.parse().ok());

        match (target_rate, key_space) {
            (Some(rate), _) => {
                let ks = collision::key_space_for_collision_rate(rate, batch_size);
                let actual = collision::expected_collision_rate(ks, batch_size);
                println!(
                    "target={:.1}%  batch_size={}  →  key_space_size={}  (actual≈{:.2}%)",
                    rate * 100.0,
                    batch_size,
                    ks,
                    actual * 100.0
                );
            }
            (None, Some(ks)) => {
                let actual = collision::expected_collision_rate(ks, batch_size);
                println!(
                    "key_space_size={}  batch_size={}  →  collision_rate≈{:.2}%",
                    ks,
                    batch_size,
                    actual * 100.0
                );
            }
            (None, None) => {
                collision::print_collision_table(batch_size);
            }
        }
        return;
    }

    let is_client = std::env::var("CLIENT").map(|x| x == "1").unwrap_or(false);

    let runtime_config = atlas_default_configs::get_runtime_configuration()
        .expect("Failed to get runtime configurations");

    let RunTimeSettings {
        threadpool_threads,
        async_runtime_threads,
    } = runtime_config;

    let conf = InitConfig {
        threadpool_threads,
        async_threads: async_runtime_threads,
    };

    let _guard = unsafe { init(conf).unwrap() };

    if !is_client {
        replica::run_replica();
    } else {
        client::client_main();
    }
}
