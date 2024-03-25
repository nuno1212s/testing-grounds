#![feature(alloc_error_hook)]

use std::alloc::{set_alloc_error_hook, Layout};

mod exec;
mod metric;
mod serialize;

mod bench;
mod client;
mod common;
mod cop;
mod os_statistics;
mod workload_gen;

// #[cfg(not(target_env = "msvc"))]
// use tikv_jemallocator::Jemalloc;

// #[cfg(not(target_env = "msvc"))]
// #[global_allocator]
// static GLOBAL: Jemalloc = Jemalloc;

fn custom_alloc_error_hook(layout: Layout) {
    panic!("allocation error: {:?} bytes", layout.size())
}

fn main() {
    set_alloc_error_hook(custom_alloc_error_hook);

    cop::main()
}
