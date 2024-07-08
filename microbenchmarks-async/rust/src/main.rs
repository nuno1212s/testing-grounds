#![feature(alloc_error_hook)]

use std::alloc::Layout;

use atlas_common::{init, InitConfig};
use atlas_default_configs::runtime_settings::RunTimeSettings;

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
    let is_client = std::env::var("CLIENT")
        .map(|x| x == "1")
        .unwrap_or(false);

    let runtime_config = atlas_default_configs::get_runtime_configuration().expect("Failed to get runtime configurations");

    let RunTimeSettings {
        threadpool_threads,
        async_runtime_threads
    } = runtime_config;

    let conf = InitConfig {
        //If we are the client, we want to have many threads to send stuff to replicas
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
