pub mod nodes;
pub mod params;
pub mod runtime;
pub mod handlers;

use nodes::tcp_sync;
use nodes::tcp_tokio;
use nodes::tcp_async_std;
use nodes::{
    Client, Server,
    AsyncClient, AsyncServer,
};
use runtime::{
    Runtime,
    tokio::Runtime as TRuntime,
    async_std::Runtime as ASRuntime,
};

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
        _ => invalid_backend(),
    }
}

fn kind_2(arg: &str) -> Result<(), Box<dyn std::error::Error>> {
    match arg {
        "tcp:sync:client" => {
            handlers::client_test2_sync(|| {
                let client = tcp_sync::C::connect_server(params::N1)?;
                Ok(client)
            })
        },
        "tcp:sync:server" => doit!(|| {
            let server = tcp_sync::S::listen_clients(params::LADDR)?;
            let ops = handlers::server_test2_sync(server)?;
            println!("{} requests per second", ops);
            Ok(())
        }),
        "tcp:tokio:client" => {
            handlers::client_test2_async(TRuntime, || async {
                let client = tcp_tokio::C::connect_server_async(params::N1).await?;
                Ok(client)
            })
        },
        "tcp:tokio:server" => TRuntime::block_on(async {
            let server = tcp_tokio::S::listen_clients_async(params::LADDR).await?;
            let ops = handlers::server_test2_async(TRuntime, server).await?;
            println!("{} requests per second", ops);
            Ok(())
        }),
        "tcp:async_std:client" => {
            ASRuntime::init();
            handlers::client_test2_async(ASRuntime, || async {
                let client = tcp_async_std::C::connect_server_async(params::N1).await?;
                Ok(client)
            })
        },
        "tcp:async_std:server" => {
            ASRuntime::init();
            ASRuntime::block_on(async {
                let server = tcp_async_std::S::listen_clients_async(params::LADDR).await?;
                let ops = handlers::server_test2_async(ASRuntime, server).await?;
                println!("{} requests per second", ops);
                Ok(())
            })
        },
        _ => invalid_backend(),
    }
}

fn invalid_backend() -> Result<(), Box<dyn std::error::Error>> {
    Err("Invalid backend; try \"help\".".into())
}

fn usage() -> ! {
    eprintln!("Available backends:");
    eprintln!("  - tcp:sync:{{client, server}}");
    eprintln!("  - tcp:tokio:{{client, server}}");
    eprintln!("  - tcp:async_std:{{client, server}}");
    eprintln!("");
    eprintln!("Available test kinds:");
    eprintln!("  - 1 / 2");
    std::process::exit(1)
}
