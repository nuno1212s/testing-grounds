mod app;
mod client;
mod variables;

use std::sync::Arc;
use std::{env, thread};
use std::time::Instant;
use bytes::Bytes;
use clap::Parser;
use crossbeam_utils::atomic::AtomicCell;
use crossbeam_utils::sync::WaitGroup;
use futures_util::try_join;
use themis_client::Client;
use themis_core::authentication::{create_crypto_replicas, Cryptography};
use themis_core::config::{Config, DEFAULT_CONFIG_PATH, Execution, load_from_paths, Peer, setup_logging};
use themis_core::{modules, comms as channel};
use themis_core::execute::Runtime;
use themis_pbft::PBFT;
use tracing::info;
use tracing_subscriber::EnvFilter;
use crate::app::Microbenchmark;

async fn setup(config: Arc<Config>) {
    let me: usize = config.get("id").expect("id");
    let _keyfile: String = config
        .get(&format!("peers[{}].private_key", me))
        .expect("keyfile");
    let _peers: Vec<Peer> = config.get("peers").expect("peers");

    let crypto = create_crypto_replicas(&config);
    let verifier = crypto.create_signer(&Bytes::new());

    let peer_out = channel::unbounded();
    let peer_in = channel::unbounded();
    let peers = modules::peers(peer_in.0, peer_out.1, crypto, config.clone(), None);

    let client_out = channel::unbounded();
    let client_in = channel::unbounded();
    let clients = modules::clients(client_in.0.clone(), client_out.1, config.clone());

    let app_in = channel::unbounded();
    let app_out = channel::unbounded();

    let protocol = PBFT::new(
        me as u64,
        peer_out.0,
        client_out.0,
        app_in.0,
        verifier,
        &config,
    );
    let protocol = modules::protocol(protocol, peer_in.1, client_in.1, app_out.1, &config);

    let app = Microbenchmark::new(me as u64);
    let application = modules::application(me as u64, app, app_in.1, app_out.0);

    #[cfg(feature = "metrics")]
        let port: u16 = config
        .get(&format!("peers[{}].prometheus_port", me))
        .expect("prometheus port");
    #[cfg(feature = "metrics")]
        let _metrics = spawn(start_server(port));

    info!("setup modules");

    let state = try_join!(peers, clients, protocol, application);
    if let Err(e) = state {
        log::error!("Fatal: {}", e);
    }
}

#[derive(Debug, Parser)]
struct Opts {
    #[clap(long = "config", short, default_value = DEFAULT_CONFIG_PATH)]
    configs: Vec<String>,
    id: u64,
    #[arg(short, long)]
    client: bool,
}

fn main() {
    setup_logging();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_ansi(atty::is(atty::Stream::Stdout))
        .init();

    let Opts { configs, id: me, client } = Opts::parse();

    let mut config = load_from_paths(&configs).expect("load config");
    config.set("id", me).expect("set id");

    if client {

        log::info!("STARTING CLIENTS ");


        client::start_clients(config, variables::client_count());

    } else {
        let config = Arc::new(config);
        log::info!("STARTING REPLICA {}", me);

        let rt = config.get("execution").unwrap_or(Execution::Threadpool);
        log::info!("selected runtime {:?}", rt);
        let mut rt = match rt {
            Execution::Single => tokio::runtime::Builder::new_current_thread(),
            Execution::Threadpool => tokio::runtime::Builder::new_multi_thread(),
        };
        let rt = rt.enable_time().enable_io().build().unwrap();
        rt.block_on(async move { setup(config).await })
    }
}
