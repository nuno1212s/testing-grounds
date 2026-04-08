mod exec;
mod serialize;

mod cop;
mod local;
mod common;
mod os_statistics;

#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    let is_local = std::env::var("LOCAL")
        .map(|x| x == "1")
        .unwrap_or(false);

    println!("Starting local? {}", is_local);

    if is_local {
        local::main()
    } else {
        cop::main()
    }
}
