#![feature(alloc_error_hook)]

use std::alloc::{set_alloc_error_hook, Layout};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tracing::warn;

mod exec;
mod metric;
mod serialize;

mod bench;
mod client;
mod common;
mod cop;
mod os_statistics;
mod workload_gen;

lazy_static::lazy_static! {
    static ref CANCELED: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
}

// #[cfg(not(target_env = "msvc"))]
// use tikv_jemallocator::Jemalloc;

// #[cfg(not(target_env = "msvc"))]
// #[global_allocator]
// static GLOBAL: Jemalloc = Jemalloc;
fn main() {
    /*ctrlc::set_handler(|| {
        warn!("Ctrl-C received, shutting down");
        
        CANCELED.store(true, std::sync::atomic::Ordering::SeqCst);
    }).expect("Failed to set Ctrl-C handler");*/
    
    cop::main()
}
