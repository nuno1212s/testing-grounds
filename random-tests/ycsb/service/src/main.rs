mod data;
mod exec;
mod serialize;

mod cop;
mod local;
mod common;

fn main() {
    let is_local = std::env::var("LOCAL")
        .map(|x| x == "1")
        .unwrap_or(false);

    if is_local {
        local::main()
    } else {
        cop::main()
    }
}
