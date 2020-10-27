pub mod nodes;
pub mod params;
pub mod handlers;

mod tcp_sync;

fn main() {
    let arg = match std::env::args().nth(1) {
        Some(arg) => arg,
        None => usage(1),
    };
    let result = match arg.as_ref() {
        "help" => usage(0),
        "tcp:sync:client" => tcp_sync::client(),
        "tcp:sync:server" => tcp_sync::server(),
        _ => Err("Invalid backend; try \"help\".".into()),
    };
    result.unwrap_or_else(|e| {
        eprintln!("Something went wrong: {}", e);
        std::process::exit(1);
    });
}

fn usage(code: i32) -> ! {
    eprintln!("Available backends: ");
    eprintln!("");
    eprintln!("  - tcp:sync:client");
    eprintln!("  - tcp:sync:server");
    std::process::exit(code)
}
