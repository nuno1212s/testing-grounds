#![feature(alloc_error_hook)]

use std::alloc::{Layout, set_alloc_error_hook};

mod exec;
mod serialize;
mod metric;

mod cop;
mod common;
mod os_statistics;
mod bench;

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
