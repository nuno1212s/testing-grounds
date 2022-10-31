#![allow(dead_code)]

use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::net::SocketAddr;
use std::sync::Arc;

use intmap::IntMap;
use konst::primitive::{parse_u128, parse_usize};
use konst::unwrap_ctx;
use regex::Regex;
use rustls::{
    AllowAnyAuthenticatedClient,
    ClientConfig,
    internal::pemfile,
    RootCertStore,
    ServerConfig,
};
use febft::bft::benchmarks::CommStats;

use febft::bft::communication::{NodeConfig, NodeId, PeerAddr};
use febft::bft::core::client::{
    self,
    Client,
};
use febft::bft::core::server::{
    Replica,
    ReplicaConfig,
};
use febft::bft::crypto::signature::{
    KeyPair,
    PublicKey,
};
use febft::bft::error::*;
use febft::bft::ordering::SeqNo;
use febft::bft::threadpool;

use crate::exec::Microbenchmark;
use crate::serialize::MicrobenchmarkData;

#[macro_export]
macro_rules! addr {
    ($h:expr => $a:expr) => {{
        let addr: ::std::net::SocketAddr = $a.parse().unwrap();
        (addr, String::from($h))
    }}
}

pub struct ConfigEntry {
    pub portno: u16,
    pub rep_portno: Option<u16>,
    pub id: u32,
    pub hostname: String,
    pub ipaddr: String,
}

pub fn parse_config(path: &str) -> Option<Vec<ConfigEntry>> {
    let re = Regex::new("([^ ]+)").ok()?;

    let file = File::open(path).ok()?;
    let mut file = BufReader::new(file);

    let mut buf = String::new();
    let mut config = Vec::new();

    loop {
        match file.read_line(&mut buf) {
            Ok(0) | Err(_) => break,
            _ => {
                match parse_entry(&re, &buf) {
                    Some(entry) => config.push(entry),
                    None => (),
                }
                buf.clear();
            }
        }
    }

    Some(config)
}

fn parse_entry(re: &Regex, line: &str) -> Option<ConfigEntry> {
    let line = line.trim();

    if line.chars().next() == Some('#') {
        return None;
    }

    let matches: Vec<_> = re
        .find_iter(line)
        .collect();

    if matches.len() < 4 {
        return None;
    }

    let id: u32 = matches[0].as_str().parse().ok()?;
    let hostname: String = matches[1].as_str().to_string();
    let ipaddr: String = matches[2].as_str().to_string();
    let portno: u16 = matches[3].as_str().trim_end().parse().ok()?;

    let rep_portno: Option<u16> =
        if matches.len() >= 5 {
            Some(matches[4].as_str().trim_end().parse().ok()?)
        } else { None };

    Some(ConfigEntry { id, rep_portno, hostname, ipaddr, portno })
}

async fn node_config(
    n: usize,
    id: NodeId,
    sk: KeyPair,
    addrs: IntMap<PeerAddr>,
    pk: IntMap<PublicKey>,
    comm_stats: Option<Arc<CommStats>>,
) -> NodeConfig {
    // read TLS configs concurrently
    let (client_config, server_config, client_config_replica, server_config_replica, batch_size,
    batch_timeout, batch_sleep, clients_per_pool) = {
        let cli = get_client_config(id);
        let srv = get_server_config(id);
        let cli_rustls = get_client_config_replica(id);
        let srv_rustls = get_server_config_replica(id);
        let batch_size = get_batch_size();
        let batch_timeout = get_batch_timeout();
        let batch_sleep = get_batch_sleep();
        let clients_per_pool = get_clients_per_pool();
        futures::join!(cli, srv, cli_rustls, srv_rustls, batch_size,
            batch_timeout, batch_sleep, clients_per_pool)
    };

    // build the node conf
    NodeConfig {
        id,
        n,
        f: (n - 1) / 3,
        sk,
        pk,
        addrs,
        //bind_connection_addrs: None,
        async_client_config: client_config,
        async_server_config: server_config,
        sync_client_config: client_config_replica,
        sync_server_config: server_config_replica,
        first_cli: NodeId::from(1000u32),
        batch_size,
        clients_per_pool,
        batch_timeout_micros: batch_timeout as u64,
        batch_sleep_micros: batch_sleep as u64,
        comm_stats,
    }
}

pub async fn setup_client(
    n: usize,
    id: NodeId,
    sk: KeyPair,
    addrs: IntMap<PeerAddr>,
    pk: IntMap<PublicKey>,
    comm_stats: Option<Arc<CommStats>>,
) -> Result<Client<MicrobenchmarkData>> {
    let node = node_config(n, id, sk, addrs, pk, comm_stats).await;
    let conf = client::ClientConfig {
        node,
    };
    Client::bootstrap(conf).await
}

pub async fn setup_replica(
    n: usize,
    id: NodeId,
    sk: KeyPair,
    addrs: IntMap<PeerAddr>,
    pk: IntMap<PublicKey>,
    comm_stats: Option<Arc<CommStats>>,
) -> Result<Replica<Microbenchmark>> {
    let node_id = id.clone();

    let (node, global_batch_size, global_batch_timeout) = {
        let n = node_config(n, id, sk, addrs, pk, comm_stats);
        let b = get_global_batch_size();
        let timeout = get_global_batch_timeout();
        futures::join!(n, b, timeout)
    };

    let conf = ReplicaConfig {
        node,
        global_batch_size,
        view: SeqNo::ZERO,
        next_consensus_seq: SeqNo::ZERO,
        service: Microbenchmark::new(node_id),
        batch_timeout: global_batch_timeout,
    };

    Replica::bootstrap(conf).await
}

async fn get_batch_size() -> usize {
    let (tx, rx) = oneshot::channel();
    threadpool::execute(move || {
        let mut buf = String::new();
        let mut f = open_file("./config/batch.config");
        f.read_to_string(&mut buf).unwrap();
        tx.send(buf.trim().parse().unwrap()).unwrap();
    });
    rx.await.unwrap()
}

async fn get_global_batch_size() -> usize {
    let (tx, rx) = oneshot::channel();
    threadpool::execute(move || {
        let res = parse_usize(&*std::env::var("GLOBAL_BATCH_SIZE")
            .expect("Failed to find required env var GLOBAL_BATCH_SIZE"));

        tx.send(unwrap_ctx!(res)).expect("Failed to send");
    });
    rx.await.unwrap()
}

async fn get_global_batch_timeout() -> u128 {
    let (tx, rx) = oneshot::channel();
    threadpool::execute(move || {
        let res = parse_u128(&*std::env::var("GLOBAL_BATCH_SLEEP_MICROS")
            .expect("Failed to find required env var GLOBAL_BATCH_SLEEP_MICROS"));

        tx.send(unwrap_ctx!(res)).expect("Failed to send");
    });
    rx.await.unwrap()
}

async fn get_batch_timeout() -> u128 {
    let (tx, rx) = oneshot::channel();
    threadpool::execute(move || {
        let res = parse_u128(&*std::env::var("BATCH_TIMEOUT_MICROS")
            .expect("Failed to find required env var BATCH_TIMEOUT_MICROS"));

        tx.send(unwrap_ctx!(res)).expect("Failed to send");
    });
    rx.await.unwrap()
}

async fn get_batch_sleep() -> u128 {
    let (tx, rx) = oneshot::channel();
    threadpool::execute(move || {
        let res = parse_u128(&*std::env::var("BATCH_SLEEP_MICROS")
            .expect("Failed to find required env var BATCH_SLEEP_MICROS"));

        tx.send(unwrap_ctx!(res)).expect("Failed to send");
    });
    rx.await.unwrap()
}

async fn get_clients_per_pool() -> usize {
    let (tx, rx) = oneshot::channel();
    threadpool::execute(move || {
        let res = parse_usize(&*std::env::var("CLIENTS_PER_POOL")
            .expect("Failed to find required env var CLIENTS_PER_POOL"));

        tx.send(unwrap_ctx!(res)).expect("Failed to send");
    });
    rx.await.unwrap()
}

async fn get_server_config(id: NodeId) -> ServerConfig {
    let (tx, rx) = oneshot::channel();
    threadpool::execute(move || {
        let id = usize::from(id);
        let mut root_store = RootCertStore::empty();

        // read ca file
        let certs = {
            let mut file = open_file("./ca-root/crt");
            pemfile::certs(&mut file).expect("root cert")
        };
        root_store.add(&certs[0]).unwrap();

        // create server conf
        let auth = AllowAnyAuthenticatedClient::new(root_store);
        let mut cfg = ServerConfig::new(auth);

        // configure our cert chain and secret key
        let sk = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/key", id))
            } else {
                open_file(&format!("./ca-root/cli{}/key", id))
            };
            let mut sk = pemfile::rsa_private_keys(&mut file).expect("secret key");
            sk.remove(0)
        };
        let chain = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/crt", id))
            } else {
                open_file(&format!("./ca-root/cli{}/crt", id))
            };
            let mut c = pemfile::certs(&mut file).expect("srv cert");
            c.extend(certs);
            c
        };
        cfg.set_single_cert(chain, sk).unwrap();

        tx.send(cfg).unwrap();
    });
    rx.await.unwrap()
}

async fn get_server_config_replica(id: NodeId) -> rustls::ServerConfig {
    let (tx, rx) = oneshot::channel();
    threadpool::execute(move || {
        let id = usize::from(id);
        let mut root_store = RootCertStore::empty();

        // read ca file
        let certs = {
            let mut file = open_file("./ca-root/crt");
            pemfile::certs(&mut file).expect("root cert")
        };
        root_store.add(&certs[0]).unwrap();

        // create server conf
        let auth = AllowAnyAuthenticatedClient::new(root_store);
        let mut cfg = rustls::ServerConfig::new(auth);

        // configure our cert chain and secret key
        let sk = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/key", id))
            } else {
                open_file(&format!("./ca-root/cli{}/key", id))
            };
            let mut sk = pemfile::rsa_private_keys(&mut file).expect("secret key");
            sk.remove(0)
        };
        let chain = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/crt", id))
            } else {
                open_file(&format!("./ca-root/cli{}/crt", id))
            };
            let mut c = pemfile::certs(&mut file).expect("srv cert");
            c.extend(certs);
            c
        };
        cfg.set_single_cert(chain, sk).unwrap();

        tx.send(cfg).unwrap();
    });
    rx.await.unwrap()
}

async fn get_client_config(id: NodeId) -> ClientConfig {
    let (tx, rx) = oneshot::channel();
    threadpool::execute(move || {
        let id = usize::from(id);
        let mut cfg = ClientConfig::new();

        // configure ca file
        let certs = {
            let mut file = open_file("./ca-root/crt");
            pemfile::certs(&mut file).expect("root cert")
        };
        cfg.root_store.add(&certs[0]).unwrap();

        // configure our cert chain and secret key
        let sk = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/key", id))
            } else {
                open_file(&format!("./ca-root/cli{}/key", id))
            };
            let mut sk = pemfile::rsa_private_keys(&mut file).expect("secret key");
            sk.remove(0)
        };
        let chain = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/crt", id))
            } else {
                open_file(&format!("./ca-root/cli{}/crt", id))
            };
            let mut c = pemfile::certs(&mut file).expect("srv cert");
            c.extend(certs);
            c
        };
        cfg.set_single_client_cert(chain, sk).unwrap();

        tx.send(cfg).unwrap();
    });
    rx.await.unwrap()
}

async fn get_client_config_replica(id: NodeId) -> rustls::ClientConfig {
    let (tx, rx) = oneshot::channel();

    threadpool::execute(move || {
        let id = usize::from(id);
        let mut cfg = rustls::ClientConfig::new();

        // configure ca file
        let certs = {
            let mut file = open_file("./ca-root/crt");
            pemfile::certs(&mut file).expect("root cert")
        };

        cfg.root_store.add(&certs[0]).unwrap();

        // configure our cert chain and secret key
        let sk = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/key", id))
            } else {
                open_file(&format!("./ca-root/cli{}/key", id))
            };
            let mut sk = pemfile::rsa_private_keys(&mut file).expect("secret key");
            sk.remove(0)
        };
        let chain = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/crt", id))
            } else {
                open_file(&format!("./ca-root/cli{}/crt", id))
            };
            let mut c = pemfile::certs(&mut file).expect("srv cert");
            c.extend(certs);
            c
        };
        cfg.set_single_client_cert(chain, sk).unwrap();

        tx.send(cfg).unwrap();
    });

    rx.await.unwrap()
}


fn open_file(path: &str) -> BufReader<File> {
    let file = File::open(path).expect(path);
    BufReader::new(file)
}
