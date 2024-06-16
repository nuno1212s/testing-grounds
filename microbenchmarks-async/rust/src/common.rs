#![allow(dead_code)]

use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::iter;
use std::net::ToSocketAddrs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use intmap::IntMap;
use konst::primitive::{parse_u128, parse_u32, parse_usize};
use konst::unwrap_ctx;
use pki_types::CertificateDer;
use regex::Regex;
use rustls::{ClientConfig, pki_types, RootCertStore, ServerConfig};
use rustls::pki_types::PrivateKeyDer;
use rustls_pemfile::{Item, read_one};

use atlas_client::client;
use atlas_client::client::Client;
use atlas_client::client::unordered_client::UnorderedClientMode;
use atlas_comm_mio::{ByteStubType, MIOTCPNode};
use atlas_comm_mio::config::{MIOConfig, TcpConfig, TlsConfig};
use atlas_common::crypto::signature::{KeyPair, PublicKey};
use atlas_common::error::*;
use atlas_common::node_id::{NodeId, NodeType};
use atlas_common::ordering::SeqNo;
use atlas_common::peer_addr::PeerAddr;
use atlas_common::threadpool;
use atlas_communication::{NodeInputStub, NodeStubController};
use atlas_communication::config::ClientPoolConfig;
use atlas_communication::reconfiguration::NodeInfo;
use atlas_core::serialize::NoProtocol;
use atlas_decision_log::config::DecLogConfig;
use atlas_decision_log::Log;
use atlas_decision_log::serialize::LogSerialization;
use atlas_log_transfer::CollabLogTransfer;
use atlas_log_transfer::config::LogTransferConfig;
use atlas_log_transfer::messages::serialize::LTMsg;
use atlas_metrics::benchmarks::CommStats;
use atlas_metrics::InfluxDBArgs;
use atlas_persistent_log::stateful_logs::monolithic_state::MonStatePersistentLog;
use atlas_reconfiguration::config::ReconfigurableNetworkConfig;
use atlas_reconfiguration::message::ReconfData;
use atlas_reconfiguration::network_reconfig::NetworkInfo;
use atlas_reconfiguration::ReconfigurableNodeProtocolHandle;
use atlas_smr_core::networking::{ReplicaNodeWrapper, SMRReplicaNetworkNode};
use atlas_smr_core::networking::client::{CLINodeWrapper, SMRClientNetworkNode};
use atlas_smr_core::request_pre_processing::RequestPreProcessor;
use atlas_smr_core::serialize::{Service, SMRSysMsg, StateSys};
use atlas_smr_core::SMRReq;
use atlas_smr_execution::SingleThreadedMonExecutor;
use atlas_smr_replica::config::{MonolithicStateReplicaConfig, ReplicaConfig};
use atlas_smr_replica::server::Exec;
use atlas_smr_replica::server::monolithic_server::MonReplica;
use atlas_view_transfer::config::ViewTransferConfig;
use atlas_view_transfer::message::serialize::ViewTransfer;
use atlas_view_transfer::SimpleViewTransferProtocol;
use febft_pbft_consensus::bft::config::{PBFTConfig, ProposerConfig};
use febft_pbft_consensus::bft::message::serialize::PBFTConsensus;
use febft_pbft_consensus::bft::PBFTOrderProtocol;
use febft_pbft_consensus::bft::sync::view::ViewInfo;
use febft_state_transfer::CollabStateTransfer;
use febft_state_transfer::config::StateTransferConfig;
use febft_state_transfer::message::serialize::CSTMsg;

use crate::exec::Microbenchmark;
use crate::serialize::{MicrobenchmarkData, State};

#[macro_export]
macro_rules! addr {
    ($h:expr => $a:expr) => {{
        let server : Vec<_> = ::std::net::ToSocketAddrs::to_socket_addrs($a)
        .expect("Unable to resolve domain")
        .collect();

        let addr: ::std::net::SocketAddr = server.into_iter().next().expect("Resolved domain has no corresponding IPs?");
        (addr, String::from($h))
    }}
}

/*
pub struct ObserverCall;

impl ObserverCallback for ObserverCall {
    fn handle_event(&self, event: ObserveEventKind, n: usize) {
        match event {
            ObserveEventKind::CheckpointStart(start_cp) => {
                println!("Received checkpoint start with seq {:?}", start_cp)
            }
            ObserveEventKind::CheckpointEnd(end_cp) => {
                println!("Received checkpoint end with seq {:?}", end_cp)
            }
            ObserveEventKind::Consensus(consensus_instance) => {
                println!("Received consensus phase with seq {:?}", consensus_instance)
            }
            ObserveEventKind::NormalPhase((view, seq)) => {
                println!("Received normal phase message with seq {:?} for view {:?} with the current leader {:?}", seq, view.sequence_number(), view.leader());
            }
            ObserveEventKind::ViewChangePhase => {
                println!("Received view change phase message")
            }
            ObserveEventKind::CollabStateTransfer => {
                println!("Received collab state transfer message")
            }
            ObserveEventKind::Prepare(seq) => {
                println!("Received prepare stage with seq {:?}", seq)
            }
            ObserveEventKind::Commit(seq) => {
                println!("Received commit stage with seq {:?}", seq)
            }
            ObserveEventKind::Ready(_) => {}
            ObserveEventKind::Executed(_) => {}
        }
    }
}

 */

pub struct ConfigEntry {
    pub portno: u16,
    pub rep_portno: Option<u16>,
    pub id: u32,
    pub hostname: String,
    pub ipaddr: String,
}

pub fn parse_config(path: &str) -> Option<Vec<ConfigEntry>> {
    let re = Regex::new("([^ ]+)").ok()?;

    println!("Opening file : {:?}", path);
    let path_parsed = Path::new(path);
    println!("path_parsed: {:?}", path_parsed.canonicalize());
    
    let file = File::open(path).ok()?;
    let mut file = BufReader::new(file);

    let mut buf = String::new();
    let mut config = Vec::new();

    loop {
        match file.read_line(&mut buf) {
            Ok(0) => {
                println!("Finalizing reading the config file {} with size {}", path, config.len());

                break;
            }
            Err(err) => {
                println!("Finalizing reading the config file {} with size {} with err {:?}", path, config.len(), err);

                break;
            }
            _ => {
                match parse_entry(&re, &buf) {
                    Some(entry) => config.push(entry),
                    None => {
                        println!("Failed to parse entry: {}", buf);
                    }
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

/// Get the configuration for influx DB
pub fn influx_db_config(id: NodeId) -> InfluxDBArgs {
    let ip = std::env::var("INFLUX_IP").expect("INFLUX_IP not set");
    let db_name = std::env::var("INFLUX_DB_NAME").expect("INFLUX_DB_NAME not set");
    let user = std::env::var("INFLUX_USER").expect("INFLUX_USER not set");
    let password = std::env::var("INFLUX_PASSWORD").expect("INFLUX_PASSWORD not set");

    let extra = std::env::var("INFLUX_EXTRA_TEST").unwrap_or("".to_string());

    let extra = Some(format!("test-{}", extra));

    InfluxDBArgs {
        ip,
        db_name,
        user,
        password,
        node_id: id,
        extra,
    }
}

const BOOSTRAP_NODES: [u32; 4] = [0, 1, 2, 3];

async fn node_config(
    n: usize,
    id: NodeId,
    comm_stats: Option<Arc<CommStats>>,
) -> (MIOConfig, ClientPoolConfig) {
    let first_cli = NodeId::from(1000u32);

    let mio_worker_count = std::env::var("MIO_WORKER_COUNT").unwrap_or("1".to_string()).parse::<usize>().unwrap();

    // read TLS configs concurrently
    let (client_config, server_config,
        client_config_replica, server_config_replica,
        batch_size, batch_timeout, batch_sleep, clients_per_pool) = {
        let cli = get_client_config(id);
        let srv = get_tls_sync_server_config(id);
        let cli_rustls = get_client_config_replica(id);
        let srv_rustls = get_server_config_replica(id);
        let batch_size = get_batch_size();
        let batch_timeout = get_batch_timeout();
        let batch_sleep = get_batch_sleep();
        let clients_per_pool = get_clients_per_pool();
        futures::join!(cli, srv, cli_rustls, srv_rustls, batch_size,
            batch_timeout, batch_sleep, clients_per_pool)
    };

    let tcp = TcpConfig {
        network_config: TlsConfig {
            async_client_config: client_config,
            async_server_config: server_config,
            sync_client_config: client_config_replica,
            sync_server_config: server_config_replica,
        },
        replica_concurrent_connections: 1,
        client_concurrent_connections: 1,
    };

    let cp = ClientPoolConfig::new(batch_size, 100, clients_per_pool, batch_timeout as u64, batch_sleep as u64, 100);

    let mio_cfg = MIOConfig {
        tcp_configs: tcp,
        epoll_worker_count: mio_worker_count as u32,
    };

    (mio_cfg, cp)
}

/// Set up the data handles so we initialize the networking layer
pub type ReconfigurationMessage = ReconfData;

/// In the case of SMR messages, we want the type that is going to be ordered to include just the actual
/// SMR Ordered Request Type, so we can use the same type for the ordering protocol
/// This type, for SMR is [atlas_smr_core::serialize::SMRReq]
///
/// These protocols are only going to be used for the ordered requests, so they only have to know about the ordered requests
/// In further parts, we can utilize [MicrobenchmarkData] directly as it requires a [D: ApplicationData], instead of just [SerType]
pub type OrderProtocolMessage = PBFTConsensus<SMRReq<MicrobenchmarkData>>;
pub type DecLogMsg = LogSerialization<SMRReq<MicrobenchmarkData>, OrderProtocolMessage, OrderProtocolMessage>;
pub type LogTransferMessage = LTMsg<SMRReq<MicrobenchmarkData>, OrderProtocolMessage, OrderProtocolMessage, DecLogMsg>;
pub type ViewTransferMessage = ViewTransfer<OrderProtocolMessage>;

/// The state transfer also requires wrapping in order to keep the [atlas_communication::serialization::Serializable] type
/// out of the state transfer protocol (and all others for that matter) for further flexibility
/// Therefore, we have to wrap the [atlas_smr_core::serialize::StateSys] type in order to get the [atlas_communication::serialization::Serializable] trait
///
pub type StateTransferMessage = CSTMsg<State>;
pub type SerStateTransferMessage = StateSys<StateTransferMessage>;


/// This type is the protocol type responsible for all SMR messages including unordered ones, so it already knows about [atlas_smr_application::ApplicationData]
pub type ProtocolDataType = Service<MicrobenchmarkData, OrderProtocolMessage, LogTransferMessage, ViewTransferMessage>;

/// Set up the networking layer with the data handles we have
///
/// In the networking level, we utilize the type which wraps [atlas_smr_application::ApplicationData]
/// and provides the [atlas_communication::serialization::Serializable] type required
/// for the network layer.
///
/// For that, we use [atlas_smr_core::serialize::SMRSysMsg]

/// Replica stub things
pub type IncomingStub = NodeInputStub<ReconfigurationMessage, ProtocolDataType, SerStateTransferMessage, SMRSysMsg<MicrobenchmarkData>>;
pub type StubController = NodeStubController<NetworkInfo, ByteStubType, ReconfigurationMessage, ProtocolDataType, SerStateTransferMessage, SMRSysMsg<MicrobenchmarkData>>;

pub type ByteNetworkLayer = MIOTCPNode<NetworkInfo, IncomingStub, StubController>;

pub type ReplicaNode = ReplicaNodeWrapper<ByteStubType, ByteNetworkLayer, NetworkInfo, ReconfigurationMessage, MicrobenchmarkData, OrderProtocolMessage,
    LogTransferMessage, ViewTransferMessage, StateTransferMessage>;

pub type ProtocolNetwork = <ReplicaNode as SMRReplicaNetworkNode<NetworkInfo, ReconfigurationMessage, MicrobenchmarkData, OrderProtocolMessage,
    LogTransferMessage, ViewTransferMessage, StateTransferMessage>>::ProtocolNode;

pub type StateTransferNetwork = <ReplicaNode as SMRReplicaNetworkNode<NetworkInfo, ReconfigurationMessage, MicrobenchmarkData, OrderProtocolMessage,
    LogTransferMessage, ViewTransferMessage, StateTransferMessage>>::StateTransferNode;

pub type AppNetwork = <ReplicaNode as SMRReplicaNetworkNode<NetworkInfo, ReconfigurationMessage, MicrobenchmarkData, OrderProtocolMessage,
    LogTransferMessage, ViewTransferMessage, StateTransferMessage>>::ApplicationNode;

pub type ReconfigurationNode = <ReplicaNode as SMRReplicaNetworkNode<NetworkInfo, ReconfigurationMessage, MicrobenchmarkData, OrderProtocolMessage,
    LogTransferMessage, ViewTransferMessage, StateTransferMessage>>::ReconfigurationNode;

/// Client network node stuff

pub type CLIIncomingStub = NodeInputStub<ReconfigurationMessage, NoProtocol, NoProtocol, SMRSysMsg<MicrobenchmarkData>>;
pub type CLIStubController = NodeStubController<NetworkInfo, ByteStubType, ReconfigurationMessage, NoProtocol, NoProtocol, SMRSysMsg<MicrobenchmarkData>>;

pub type CLIByteNetworkLayer = MIOTCPNode<NetworkInfo, CLIIncomingStub, CLIStubController>;

pub type ClientNode = CLINodeWrapper<ByteStubType, CLIByteNetworkLayer, NetworkInfo, ReconfigurationMessage, MicrobenchmarkData>;

pub type ClientNetwork = <ClientNode as SMRClientNetworkNode<NetworkInfo, ReconfigurationMessage, MicrobenchmarkData>>::AppNode;

/// Set up the persistent logging type with the existing data handles
pub type Logging = MonStatePersistentLog<State, MicrobenchmarkData, OrderProtocolMessage, OrderProtocolMessage, DecLogMsg, StateTransferMessage>;

/// Set up the protocols with the types that have been built up to here
pub type ReconfProtocol = ReconfigurableNodeProtocolHandle;
pub type OrderProtocol = PBFTOrderProtocol<SMRReq<MicrobenchmarkData>, RequestPreProcessor<SMRReq<MicrobenchmarkData>>, ProtocolNetwork>;
pub type DecisionLog = Log<SMRReq<MicrobenchmarkData>, OrderProtocol, Logging, Exec<MicrobenchmarkData>>;
pub type LogTransferProtocol = CollabLogTransfer<SMRReq<MicrobenchmarkData>, OrderProtocol, DecisionLog, ProtocolNetwork, Logging, Exec<MicrobenchmarkData>>;
pub type ViewTransferProt = SimpleViewTransferProtocol<OrderProtocol, ProtocolNetwork>;
pub type StateTransferProtocol = CollabStateTransfer<State, StateTransferNetwork, Logging>;

pub type SMRReplica = MonReplica<ReconfProtocol, SingleThreadedMonExecutor, State, Microbenchmark,
    OrderProtocol, DecisionLog, StateTransferProtocol, LogTransferProtocol,
    ViewTransferProt, ReplicaNode, Logging>;

pub type SMRClient = Client<ReconfProtocol, MicrobenchmarkData, ClientNetwork>;

pub fn setup_reconf(id: NodeId, sk: KeyPair, addrs: IntMap<PeerAddr>, pk: IntMap<PublicKey>, node_type: NodeType) -> Result<ReconfigurableNetworkConfig> {
    let own_addr = addrs.get(id.0 as u64).cloned().unwrap();

    let mut known_nodes = Vec::new();

    for boostrap_node in BOOSTRAP_NODES {
        let boostrap_node_id = NodeId::from(boostrap_node);

        let boostrap_addr = addrs.get(boostrap_node as u64).cloned().unwrap();

        let boostrap_pk = pk.get(boostrap_node as u64).unwrap().clone();

        known_nodes.push(NodeInfo::new(boostrap_node_id, NodeType::Replica, boostrap_pk, boostrap_addr));
    }

    println!("Known nodes: {:?}", known_nodes);

    Ok(ReconfigurableNetworkConfig {
        node_id: id,
        node_type,
        key_pair: sk,
        our_address: own_addr,
        known_nodes,
    })
}

pub async fn setup_client(
    n: usize,
    id: NodeId,
    sk: KeyPair,
    addrs: IntMap<PeerAddr>,
    pk: IntMap<PublicKey>,
    comm_stats: Option<Arc<CommStats>>,
) -> Result<SMRClient> {
    let reconf = setup_reconf(id, sk, addrs, pk, NodeType::Client)?;

    let (node, client) = node_config(n, id, comm_stats).await;

    let conf = client::ClientConfig {
        unordered_rq_mode: UnorderedClientMode::BFT,
        node,
        reconfiguration: reconf,
    };

    client::bootstrap_client::<ReconfProtocol, MicrobenchmarkData, ClientNode, OrderProtocol>(id, conf).await
}

pub async fn setup_replica(
    n: usize,
    id: NodeId,
    sk: KeyPair,
    addrs: IntMap<PeerAddr>,
    pk: IntMap<PublicKey>,
    comm_stats: Option<Arc<CommStats>>,
) -> Result<SMRReplica> {
    let node_id = id.clone();
    let db_path = format!("PERSISTENT_DB_{:?}", id);

    let reconf_config = setup_reconf(id, sk, addrs, pk, NodeType::Replica)?;

    let ((node, client_pool), global_batch_size, global_batch_timeout) = {
        let n = node_config(n, id, comm_stats);
        let b = get_global_batch_size();
        let timeout = get_global_batch_timeout();
        futures::join!(n, b, timeout)
    };

    let max_batch_size = get_max_batch_size();

    let proposer_config = ProposerConfig {
        target_batch_size: global_batch_size as u64,
        max_batch_size: global_batch_size as u64 * 2,
        batch_timeout: global_batch_timeout as u64,
    };

    let view = ViewInfo::new(SeqNo::ZERO, n, 1)?;

    let timeout_duration = Duration::from_secs(3);

    let watermark = get_watermark();

    let op_config = PBFTConfig::new(timeout_duration.clone(),
                                    watermark, proposer_config);

    let st_config = StateTransferConfig {
        timeout_duration,
    };

    let lt_config = LogTransferConfig {
        timeout_duration,
    };

    let dl_config = DecLogConfig {
        default_ongoing_capacity: watermark as usize,
    };

    let vt_config = ViewTransferConfig {
        timeout_duration,
    };

    let service = Microbenchmark::new(id);

    let conf = ReplicaConfig::<ReconfProtocol, State, MicrobenchmarkData, OrderProtocol, DecisionLog,
        StateTransferProtocol, LogTransferProtocol, ViewTransferProt, ReplicaNode, Logging> {
        node,
        next_consensus_seq: SeqNo::ZERO,
        op_config,
        dl_config,
        lt_config,
        db_path,
        pl_config: (),
        p: Default::default(),
        reconfig_node: reconf_config,
        vt_config,
    };

    let mon_conf = MonolithicStateReplicaConfig {
        service,
        replica_config: conf,
        st_config,
    };

    MonReplica::bootstrap(mon_conf).await
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

fn get_max_batch_size() -> usize {
    let res = parse_usize(&*std::env::var("MAX_BATCH_SIZE")
        .expect("Failed to find required env var MAX_BATCH_SIZE"));

    unwrap_ctx!(res)
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


pub fn get_concurrent_rqs() -> usize {
    let res = parse_usize(&*std::env::var("CONCURRENT_RQS")
        .expect("Failed to find required env var CONCURRENT_RQS"));

    unwrap_ctx!(res)
}

pub fn get_watermark() -> u32 {
    let res = parse_u32(&*std::env::var("WATERMARK")
        .expect("Failed to find required env var CONCURRENT_RQS"));

    unwrap_ctx!(res)
}

fn read_certificates_from_file(mut file: &mut BufReader<File>) -> Vec<CertificateDer<'static>> {
    let mut certs = Vec::new();

    for item in iter::from_fn(|| read_one(&mut file).transpose()) {
        match item.unwrap() {
            Item::X509Certificate(cert) => {
                certs.push(cert);
            }
            _ => {
                panic!("Key given in place of a certificate")
            }
        }
    }

    certs
}

#[inline]
fn read_private_keys_from_file(mut file: BufReader<File>) -> Vec<PrivateKeyDer<'static>> {
    let mut certs = Vec::new();

    for item in iter::from_fn(|| read_one(&mut file).transpose()) {
        match item.unwrap() {
            Item::Pkcs1Key(rsa) => {
                certs.push(PrivateKeyDer::Pkcs1(rsa))
            }
            Item::Pkcs8Key(rsa) => {
                certs.push(PrivateKeyDer::Pkcs8(rsa))
            }
            Item::Sec1Key(rsa) => {
                certs.push(PrivateKeyDer::Sec1(rsa))
            }
            _ => {
                panic!("Certificate given in place of a key")
            }
        }
    }

    certs
}

#[inline]
fn read_private_key_from_file(mut file: BufReader<File>) -> PrivateKeyDer<'static> {
    read_private_keys_from_file(file).pop().unwrap()
}

async fn get_tls_sync_server_config(id: NodeId) -> ServerConfig {
    let (tx, rx) = oneshot::channel();
    threadpool::execute(move || {
        let id = usize::from(id);
        let mut root_store = RootCertStore::empty();

        // read ca file
        let cert = {
            let mut file = open_file("./ca-root/crt");

            let certs = read_certificates_from_file(&mut file);

            root_store.add(certs[0].clone()).expect("Failed to put root store");

            certs
        };

        // configure our cert chain and secret key
        let sk = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/key", id))
            } else {
                open_file(&format!("./ca-root/cli{}/key", id))
            };

            read_private_key_from_file(file)
        };

        let chain = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/crt", id))
            } else {
                open_file(&format!("./ca-root/cli{}/crt", id))
            };

            let mut certs = read_certificates_from_file(&mut file);

            certs.extend(cert);
            certs
        };

        // create server conf
        let cfg = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(chain, sk)
            .expect("Failed to make server config for TLS");

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

            read_certificates_from_file(&mut file)
        };

        root_store.add(certs[0].clone()).unwrap();

        // configure our cert chain and secret key
        let sk = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/key", id))
            } else {
                open_file(&format!("./ca-root/cli{}/key", id))
            };

            read_private_key_from_file(file)
        };
        let chain = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/crt", id))
            } else {
                open_file(&format!("./ca-root/cli{}/crt", id))
            };

            let mut c = read_certificates_from_file(&mut file);

            c.extend(certs);

            c
        };

        // create server conf
        let cfg = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(chain, sk)
            .expect("Failed to make cfg");

        tx.send(cfg).unwrap();
    });
    rx.await.unwrap()
}

async fn get_client_config(id: NodeId) -> ClientConfig {
    let (tx, rx) = oneshot::channel();
    threadpool::execute(move || {
        let id = usize::from(id);

        let mut root_store = RootCertStore::empty();

        // configure ca file
        let certs = {
            let mut file = open_file("./ca-root/crt");
            read_certificates_from_file(&mut file)
        };

        root_store.add(certs[0].clone()).unwrap();

        // configure our cert chain and secret key
        let sk = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/key", id))
            } else {
                open_file(&format!("./ca-root/cli{}/key", id))
            };

            read_private_key_from_file(file)
        };

        let chain = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/crt", id))
            } else {
                open_file(&format!("./ca-root/cli{}/crt", id))
            };
            let mut c = read_certificates_from_file(&mut file);

            c.extend(certs);
            c
        };

        let cfg = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_client_auth_cert(chain, sk)
            .expect("bad cert/key");

        tx.send(cfg).unwrap();
    });
    rx.await.unwrap()
}

async fn get_client_config_replica(id: NodeId) -> rustls::ClientConfig {
    let (tx, rx) = oneshot::channel();

    threadpool::execute(move || {
        let id = usize::from(id);

        let mut root_store = RootCertStore::empty();

        // configure ca file
        let certs = {
            let mut file = open_file("./ca-root/crt");
            read_certificates_from_file(&mut file)
        };

        root_store.add(certs[0].clone()).unwrap();

        // configure our cert chain and secret key
        let sk = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/key", id))
            } else {
                open_file(&format!("./ca-root/cli{}/key", id))
            };

            read_private_key_from_file(file)
        };

        let chain = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/crt", id))
            } else {
                open_file(&format!("./ca-root/cli{}/crt", id))
            };
            let mut c = read_certificates_from_file(&mut file);

            c.extend(certs);
            c
        };

        let cfg = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_client_auth_cert(chain, sk)
            .expect("bad cert/key");

        tx.send(cfg).unwrap();
    });

    rx.await.unwrap()
}


fn open_file(path: &str) -> BufReader<File> {
    let file = File::open(path).expect(path);
    BufReader::new(file)
}