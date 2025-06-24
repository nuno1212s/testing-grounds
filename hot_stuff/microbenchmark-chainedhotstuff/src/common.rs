#![allow(dead_code)]

use crate::exec::Microbenchmark;
use crate::serialize::{MicrobenchmarkData, State};
use atlas_client::client::Client;
use atlas_comm_mio::{ByteStubType, MIOTCPNode};
use atlas_communication::{NodeInputStub, NodeStubController};
use atlas_core::ordering_protocol::OrderProtocolTolerance;
use atlas_core::serialize::NoProtocol;
use atlas_decision_log::serialize::LogSerialization;
use atlas_decision_log::Log;
use atlas_log_transfer::messages::serialize::LTMsg;
use atlas_log_transfer::CollabLogTransfer;
use atlas_persistent_log::stateful_logs::monolithic_state::MonStatePersistentLog;
use atlas_reconfiguration::message::ReconfData;
use atlas_reconfiguration::network_reconfig::NetworkInfo;
use atlas_reconfiguration::ReconfigurableNodeProtocolHandle;
use atlas_smr_core::networking::client::{CLINodeWrapper, SMRClientNetworkNode};
use atlas_smr_core::networking::{ReplicaNodeWrapper, SMRReplicaNetworkNode};
use atlas_smr_core::serialize::{SMRSysMsg, Service, StateSys};
use atlas_smr_core::SMRReq;
use atlas_smr_execution::SingleThreadedMonExecutor;
use atlas_smr_replica::config::{MonolithicStateReplicaConfig, ReplicaConfig};
use atlas_smr_replica::server::monolithic_server::MonReplica;
use atlas_smr_replica::server::Exec;
use atlas_view_transfer::message::serialize::ViewTransfer;
use atlas_view_transfer::SimpleViewTransferProtocol;
use febft_state_transfer::message::serialize::CSTMsg;
use febft_state_transfer::CollabStateTransfer;
use hot_iron_oxide::crypto::QuorumInfo;
use hot_iron_oxide::protocol::messages::serialize::HotIronOxSer;
use hot_iron_oxide::HotIron;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::EnvFilter;
use hot_iron_oxide::chained::IronChain;
use hot_iron_oxide::chained::messages::serialize::IronChainSer;

/// Set up the data handles so we initialize the networking layer
pub type ReconfigurationMessage = ReconfData;

/// In the case of SMR messages, we want the type that is going to be ordered to include just the actual
/// SMR Ordered Request Type, so we can use the same type for the ordering protocol
/// This type, for SMR is [`SMRReq`]
///
/// These protocols are only going to be used for the ordered requests, so they only have to know about the ordered requests
/// In further parts, we can utilize [`MicrobenchmarkData`] directly as it requires a [D: `ApplicationData`], instead of just [`SerType`]
pub type OrderProtocolMessage = IronChainSer<SMRReq<MicrobenchmarkData>>;
pub type DecLogMsg =
    LogSerialization<SMRReq<MicrobenchmarkData>, OrderProtocolMessage, OrderProtocolMessage>;
pub type LogTransferMessage =
    LTMsg<SMRReq<MicrobenchmarkData>, OrderProtocolMessage, OrderProtocolMessage, DecLogMsg>;
pub type ViewTransferMessage = ViewTransfer<OrderProtocolMessage>;

/// The state transfer also requires wrapping to keep the [`atlas_communication::serialization::SerMsg`] type
/// out of the state transfer protocol (and all others for that matter) for further flexibility
/// Therefore, we have to wrap the [`StateSys`] type to get the [`atlas_communication::serialization::SerMsg`] trait
///
pub type StateTransferMessage = CSTMsg<State>;
pub type SerStateTransferMessage = StateSys<StateTransferMessage>;

/// This type is the protocol type responsible for all SMR messages including unordered ones, so it already knows about [`atlas_smr_application::ApplicationData`]
pub type ProtocolDataType =
    Service<MicrobenchmarkData, OrderProtocolMessage, LogTransferMessage, ViewTransferMessage>;

/// Set up the networking layer with the data handles we have
///
/// In the networking level, we utilize the type which wraps [`atlas_smr_application::ApplicationData`]
/// and provides the [`atlas_communication::serialization::SerMsg`] type required
/// for the network layer.
///
/// For that, we use [`SMRSysMsg`]
///
/// Replica stub things
pub type IncomingStub = NodeInputStub<
    ReconfigurationMessage,
    ProtocolDataType,
    SerStateTransferMessage,
    SMRSysMsg<MicrobenchmarkData>,
>;
pub type StubController = NodeStubController<
    NetworkInfo,
    ByteStubType,
    ReconfigurationMessage,
    ProtocolDataType,
    SerStateTransferMessage,
    SMRSysMsg<MicrobenchmarkData>,
>;

pub type ByteNetworkLayer = MIOTCPNode<NetworkInfo, IncomingStub, StubController>;

pub type ReplicaNode = ReplicaNodeWrapper<
    ByteStubType,
    ByteNetworkLayer,
    NetworkInfo,
    ReconfigurationMessage,
    MicrobenchmarkData,
    OrderProtocolMessage,
    LogTransferMessage,
    ViewTransferMessage,
    StateTransferMessage,
>;

pub type ProtocolNetwork = <ReplicaNode as SMRReplicaNetworkNode<
    NetworkInfo,
    ReconfigurationMessage,
    MicrobenchmarkData,
    OrderProtocolMessage,
    LogTransferMessage,
    ViewTransferMessage,
    StateTransferMessage,
>>::ProtocolNode;

pub type StateTransferNetwork = <ReplicaNode as SMRReplicaNetworkNode<
    NetworkInfo,
    ReconfigurationMessage,
    MicrobenchmarkData,
    OrderProtocolMessage,
    LogTransferMessage,
    ViewTransferMessage,
    StateTransferMessage,
>>::StateTransferNode;

pub type AppNetwork = <ReplicaNode as SMRReplicaNetworkNode<
    NetworkInfo,
    ReconfigurationMessage,
    MicrobenchmarkData,
    OrderProtocolMessage,
    LogTransferMessage,
    ViewTransferMessage,
    StateTransferMessage,
>>::ApplicationNode;

pub type ReconfigurationNode = <ReplicaNode as SMRReplicaNetworkNode<
    NetworkInfo,
    ReconfigurationMessage,
    MicrobenchmarkData,
    OrderProtocolMessage,
    LogTransferMessage,
    ViewTransferMessage,
    StateTransferMessage,
>>::ReconfigurationNode;

/// Client network node stuff
pub type CLIIncomingStub =
    NodeInputStub<ReconfigurationMessage, NoProtocol, NoProtocol, SMRSysMsg<MicrobenchmarkData>>;
pub type CLIStubController = NodeStubController<
    NetworkInfo,
    ByteStubType,
    ReconfigurationMessage,
    NoProtocol,
    NoProtocol,
    SMRSysMsg<MicrobenchmarkData>,
>;

pub type CLIByteNetworkLayer = MIOTCPNode<NetworkInfo, CLIIncomingStub, CLIStubController>;

pub type ClientNode = CLINodeWrapper<
    ByteStubType,
    CLIByteNetworkLayer,
    NetworkInfo,
    ReconfigurationMessage,
    MicrobenchmarkData,
>;

pub type ClientNetwork = <ClientNode as SMRClientNetworkNode<
    NetworkInfo,
    ReconfigurationMessage,
    MicrobenchmarkData,
>>::AppNode;

/// Set up the persistent logging type with the existing data handles
pub type Logging = MonStatePersistentLog<
    State,
    MicrobenchmarkData,
    OrderProtocolMessage,
    OrderProtocolMessage,
    DecLogMsg,
    StateTransferMessage,
>;

/// Set up the protocols with the types that have been built up to here
pub type ReconfProtocol = ReconfigurableNodeProtocolHandle;
pub type OrderProtocol = IronChain<SMRReq<MicrobenchmarkData>, ProtocolNetwork, QuorumInfo>;

pub type DecisionLog =
    Log<SMRReq<MicrobenchmarkData>, OrderProtocol, Logging, Exec<MicrobenchmarkData>>;
pub type LogTransferProtocol = CollabLogTransfer<
    SMRReq<MicrobenchmarkData>,
    OrderProtocol,
    DecisionLog,
    ProtocolNetwork,
    Logging,
    Exec<MicrobenchmarkData>,
>;
pub type ViewTransferProt = SimpleViewTransferProtocol<OrderProtocol, ProtocolNetwork>;
pub type StateTransferProtocol = CollabStateTransfer<State, StateTransferNetwork, Logging>;

pub type ReplicaConf = ReplicaConfig<
    ReconfProtocol,
    State,
    MicrobenchmarkData,
    OrderProtocol,
    DecisionLog,
    StateTransferProtocol,
    LogTransferProtocol,
    ViewTransferProt,
    ReplicaNode,
    Logging,
>;
pub type MonConfig = MonolithicStateReplicaConfig<
    ReconfProtocol,
    State,
    Microbenchmark,
    OrderProtocol,
    DecisionLog,
    StateTransferProtocol,
    LogTransferProtocol,
    ViewTransferProt,
    ReplicaNode,
    Logging,
>;

pub type SMRReplica = MonReplica<
    ReconfProtocol,
    SingleThreadedMonExecutor,
    State,
    Microbenchmark,
    OrderProtocol,
    DecisionLog,
    StateTransferProtocol,
    LogTransferProtocol,
    ViewTransferProt,
    ReplicaNode,
    Logging,
>;

pub type SMRClient = Client<ReconfProtocol, MicrobenchmarkData, ClientNetwork>;

pub struct BFT;

impl OrderProtocolTolerance for BFT {
    fn get_n_for_f(f: usize) -> usize {
        3 * f + 1
    }

    fn get_quorum_for_n(n: usize) -> usize {
        Self::get_f_for_n(n) * 2 + 1
    }

    fn get_f_for_n(n: usize) -> usize {
        (n - 1) / 3
    }
}

pub fn generate_log(id: u32) -> Vec<WorkerGuard> {
    let host_folder = format!("./logs/log_{id}");

    let debug_file =
        tracing_appender::rolling::minutely(host_folder.clone(), format!("atlas_debug_{id}.log"));
    let warn_file = tracing_appender::rolling::hourly(host_folder, format!("atlas_{id}.log"));

    let (debug_file_nb, guard_1) = tracing_appender::non_blocking(debug_file);
    let (warn_file_nb, guard_2) = tracing_appender::non_blocking(warn_file);
    let (console_nb, guard_3) = tracing_appender::non_blocking(std::io::stdout());

    let debug_file_nb = debug_file_nb;
    let warn_file_nb = warn_file_nb.with_max_level(Level::INFO);
    let console_nb = console_nb.with_max_level(Level::WARN);

    let all_files = debug_file_nb.and(warn_file_nb).and(console_nb);

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        //.with_span_events(FmtSpan::ENTER | FmtSpan::EXIT)
        .json()
        .with_writer(all_files)
        .init();

    vec![guard_1, guard_2, guard_3]
}
