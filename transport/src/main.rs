pub mod nodes;
pub mod params;
pub mod handlers;

mod tcp_sync;

use nodes::{Client, Server};

macro_rules! doit {
    ($f:expr) => { $f() }
}

fn main() {
    let arg = match std::env::args().nth(1) {
        Some(arg) => arg,
        None => usage(1),
    };
    let result = match arg.as_ref() {
        "help" => usage(0),
        "tcp:sync:client" => doit!(|| {
            let clients: Result<Vec<_>, _> = params::ADDRS
                .iter()
                .skip(1)
                .map(|addr| tcp_sync::C::connect_server(addr))
                .collect();
            let ops = handlers::client_test1_sync(clients?)?;
            println!("{} requests per second", ops);
            Ok(())
        }),
        "tcp:sync:server" => doit!(|| {
            let server = tcp_sync::S::listen_clients(params::LADDR)?;
            handlers::server_test1_sync(server)
        }),
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
