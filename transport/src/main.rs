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
        None => usage(),
    };
    let result = match std::env::var("TEST").as_ref().map(String::as_ref) {
        Ok("1") => kind_1(&arg),
        Ok("2") => kind_2(&arg),
        _ => usage(),
    };
    result.unwrap_or_else(|e| {
        eprintln!("Something went wrong: {}", e);
        std::process::exit(1);
    });
}

fn kind_1(arg: &str) -> Result<(), Box<dyn std::error::Error>> {
    match arg {
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
    }
}

fn kind_2(arg: &str) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

fn usage() -> ! {
    eprintln!("Available backends:");
    eprintln!("  - tcp:sync:{{client, server}}");
    eprintln!("");
    eprintln!("Available test kinds:");
    eprintln!("  - 1 / 2");
    std::process::exit(1)
}
