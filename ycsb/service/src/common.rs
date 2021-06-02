#![allow(dead_code)]

use std::fs::File;
use std::net::SocketAddr;
use std::io::{BufReader, BufRead};

use regex::Regex;
use rustls::{
    internal::pemfile,
    ServerConfig,
    ClientConfig,
    RootCertStore,
    AllowAnyAuthenticatedClient,
};

use febft::bft::error::*;
use febft::bft::consensus::SeqNo;
use febft::bft::collections::HashMap;
use febft::bft::threadpool::ThreadPool;
use febft::bft::communication::message::{
    Message,
    SystemMessage,
};
use febft::bft::communication::{
    Node,
    NodeId,
    NodeConfig,
};
use febft::bft::crypto::signature::{
    KeyPair,
    PublicKey,
};
use febft::bft::core::client::{
    self,
    Client,
};
use febft::bft::core::server::{
    Replica,
    ReplicaConfig,
};

use crate::data::Update;
use crate::exec::YcsbService;
use crate::serialize::YcsbData;

#[macro_export]
macro_rules! addr {
    ($h:expr => $a:expr) => {{
        let addr: ::std::net::SocketAddr = $a.parse().unwrap();
        (addr, String::from($h))
    }}
}

#[macro_export]
macro_rules! map {
    ( $($key:expr => $value:expr),+ ) => {{
        let mut m = ::febft::bft::collections::hash_map();
        $(
            m.insert($key, $value);
        )+
        m
     }};
}

pub struct ConfigEntry {
    pub portno: u16,
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
                let entry = parse_entry(&re, &buf)?;
                config.push(entry);
                buf.clear();
            },
        }
    }

    Some(config)
}

fn parse_entry(re: &Regex, line: &str) -> Option<ConfigEntry> {
    let matches: Vec<_> = re
        .find_iter(line)
        .collect();

    if matches.len() != 4 || matches[0].as_str().find('#') == Some(0) {
        return None;
    }

    let id: u32 = matches[0].as_str().parse().ok()?;
    let hostname: String = matches[1].as_str().to_string();
    let ipaddr: String = matches[2].as_str().to_string();
    let portno: u16 = matches[3].as_str().trim_end().parse().ok()?;

    Some(ConfigEntry { id, hostname, ipaddr, portno })
}

pub fn debug_rogue(rogue: Vec<Message<Update, u32>>) -> String {
    let mut buf = String::new();
    buf.push_str("[ ");
    for m in rogue {
        let code = debug_msg(m);
        buf.push_str(code);
        buf.push_str(" ");
    }
    buf.push_str("]");
    buf
}

pub fn debug_msg(m: Message<Update, u32>) -> &'static str {
    match m {
        Message::System(_, m) => match m {
            SystemMessage::Request(_) => "Req",
            _ => unreachable!(),
        },
        Message::ConnectedTx(_, _) => "CTx",
        Message::ConnectedRx(_, _) => "CRx",
        Message::DisconnectedTx(_) => "DTx",
        Message::DisconnectedRx(_) => "DRx",
        Message::ExecutionFinished(_, _, _) => "Exe",
        Message::ExecutionFinishedWithAppstate(_, _, _, _) => "ExA",
    }
}

async fn node_config(
    t: &ThreadPool,
    id: NodeId,
    sk: KeyPair,
    addrs: HashMap<NodeId, (SocketAddr, String)>,
    pk: HashMap<NodeId, PublicKey>,
) -> NodeConfig {
    // read TLS configs concurrently
    let (client_config, server_config) = {
        let cli = get_client_config(t, id);
        let srv = get_server_config(t, id);
        futures::join!(cli, srv)
    };

    // build the node conf
    NodeConfig {
        id,
        n: 4,
        f: 1,
        sk,
        pk,
        addrs,
        client_config,
        server_config,
        first_cli: NodeId::from(1000u32),
    }
}

pub async fn setup_client(
    t: ThreadPool,
    id: NodeId,
    sk: KeyPair,
    addrs: HashMap<NodeId, (SocketAddr, String)>,
    pk: HashMap<NodeId, PublicKey>,
) -> Result<Client<YcsbData>> {
    let node = node_config(&t, id, sk, addrs, pk).await;
    let conf = client::ClientConfig {
        node,
    };
    Client::bootstrap(conf).await
}

pub async fn setup_replica(
    t: ThreadPool,
    id: NodeId,
    sk: KeyPair,
    addrs: HashMap<NodeId, (SocketAddr, String)>,
    pk: HashMap<NodeId, PublicKey>,
) -> Result<Replica<YcsbService>> {
    let node = node_config(&t, id, sk, addrs, pk).await;
    let conf = ReplicaConfig {
        node,
        next_consensus_seq: SeqNo::from(0),
        leader: NodeId::from(0u32),
        service: YcsbService,
    };
    Replica::bootstrap(conf).await
}

pub async fn setup_node(
    t: ThreadPool,
    id: NodeId,
    sk: KeyPair,
    addrs: HashMap<NodeId, (SocketAddr, String)>,
    pk: HashMap<NodeId, PublicKey>,
) -> Result<(Node<YcsbData>, Vec<Message<Update, u32>>)> {
    let conf = node_config(&t, id, sk, addrs, pk).await;
    Node::bootstrap(conf).await
}

async fn get_server_config(t: &ThreadPool, id: NodeId) -> ServerConfig {
    let (tx, rx) = oneshot::channel();
    t.execute(move || {
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

async fn get_client_config(t: &ThreadPool, id: NodeId) -> ClientConfig {
    let (tx, rx) = oneshot::channel();
    t.execute(move || {
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

fn open_file(path: &str) -> BufReader<File> {
    let file = File::open(path).expect(path);
    BufReader::new(file)
}
