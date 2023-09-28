#![feature(alloc_error_hook)]

use std::alloc::{Layout, set_alloc_error_hook, System};

mod exec;
mod serialize;
mod metric;

mod cop;
mod local;
mod common;
mod os_statistics;
mod bench;

// #[cfg(not(target_env = "msvc"))]
// use tikv_jemallocator::Jemalloc;

// #[cfg(not(target_env = "msvc"))]
// #[global_allocator]
// static GLOBAL: Jemalloc = Jemalloc;

//#[global_allocator]
//static GLOBAL: System = System;


fn custom_alloc_error_hook(layout: Layout) {
    panic!("allocation error: {:?} bytes", layout.size())
}

fn main() {
    let is_local = std::env::var("LOCAL")
        .map(|x| x == "1")
        .unwrap_or(false);

    println!("Starting local? {}", is_local);

    set_alloc_error_hook(custom_alloc_error_hook);

    if is_local {
        local::main()
    } else {
        cop::main()
    }
}
