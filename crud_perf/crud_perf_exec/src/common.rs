#![allow(dead_code)]

use atlas_client::client::Client;
use atlas_comm_mio::{ByteStubType, MIOTCPNode};
use atlas_communication::{NodeInputStub, NodeStubController};
use atlas_core::ordering_protocol::OrderProtocolTolerance;
use atlas_core::serialize::NoProtocol;
use atlas_decision_log::Boule;
use atlas_decision_log::serialize::LogSerialization;
use atlas_log_transfer::CollabLogTransfer;
use atlas_log_transfer::messages::serialize::LTMsg;
use atlas_persistent_log::stateful_logs::monolithic_state::MonStatePersistentLog;
use atlas_reconfiguration::ReconfigurableNodeProtocolHandle;
use atlas_reconfiguration::message::ReconfData;
use atlas_reconfiguration::network_reconfig::NetworkInfo;
use atlas_smr_core::SMRReq;
use atlas_smr_core::execution::{SMRExecWrapper, TExecutor};
use atlas_smr_core::networking::client::{CLINodeWrapper, SMRClientNetworkNode};
use atlas_smr_core::networking::{ReplicaNodeWrapper, SMRReplicaNetworkNode};
use atlas_smr_core::request_pre_processing::RequestPreProcessor;
use atlas_smr_core::serialize::{SMRSysMsg, Service, StateSys};
use atlas_smr_preemptive_execution::MonolithicPreemptiveExecutor;
use atlas_smr_replica::config::{MonolithicStateReplicaConfig, ReplicaConfig};
use atlas_smr_replica::server::monolithic_server::MonReplica;
use atlas_view_transfer::SimpleViewTransferProtocol;
use atlas_view_transfer::message::serialize::ViewTransfer;
use febft_pbft_consensus::bft::PBFTOrderProtocol;
use febft_pbft_consensus::bft::message::serialize::PBFTConsensus;
use febft_state_transfer::CollabStateTransfer;
use febft_state_transfer::message::serialize::CSTMsg;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::MakeWriterExt;

use crate::exec::Microbenchmark;
use crate::serialize::{MicrobenchmarkData, State};

pub type ReconfigurationMessage = ReconfData;

pub type OrderProtocolMessage = PBFTConsensus<SMRReq<MicrobenchmarkData>>;
pub type DecLogMsg =
    LogSerialization<SMRReq<MicrobenchmarkData>, OrderProtocolMessage, OrderProtocolMessage>;
pub type LogTransferMessage =
    LTMsg<SMRReq<MicrobenchmarkData>, OrderProtocolMessage, OrderProtocolMessage, DecLogMsg>;
pub type ViewTransferMessage = ViewTransfer<OrderProtocolMessage>;

pub type StateTransferMessage = CSTMsg<State>;
pub type SerStateTransferMessage = StateSys<StateTransferMessage>;

pub type ProtocolDataType =
    Service<MicrobenchmarkData, OrderProtocolMessage, LogTransferMessage, ViewTransferMessage>;

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

pub type Logging = MonStatePersistentLog<
    State,
    MicrobenchmarkData,
    OrderProtocolMessage,
    OrderProtocolMessage,
    DecLogMsg,
    StateTransferMessage,
>;

pub type ReconfProtocol = ReconfigurableNodeProtocolHandle;
pub type OrderProtocol = PBFTOrderProtocol<
    SMRReq<MicrobenchmarkData>,
    RequestPreProcessor<SMRReq<MicrobenchmarkData>>,
    ProtocolNetwork,
>;
pub type Executor = MonolithicPreemptiveExecutor;
pub type ExecutorHandle =
    SMRExecWrapper<<Executor as TExecutor<Microbenchmark, State>>::ExecutionHandle>;

pub type DecisionLog = Boule<SMRReq<MicrobenchmarkData>, OrderProtocol, Logging, ExecutorHandle>;
pub type LogTransferProtocol = CollabLogTransfer<
    SMRReq<MicrobenchmarkData>,
    OrderProtocol,
    DecisionLog,
    ProtocolNetwork,
    Logging,
    ExecutorHandle,
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
    Executor,
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
    let host_folder = format!("./logs/log_{}", id);

    let debug_file =
        tracing_appender::rolling::minutely(host_folder.clone(), format!("atlas_debug_{}.log", id));
    let warn_file = tracing_appender::rolling::hourly(host_folder, format!("atlas_{}.log", id));

    let (debug_file_nb, guard_1) = tracing_appender::non_blocking(debug_file);
    let (warn_file_nb, guard_2) = tracing_appender::non_blocking(warn_file);
    let (console_nb, guard_3) = tracing_appender::non_blocking(std::io::stdout());

    let warn_file_nb = warn_file_nb.with_max_level(Level::INFO);
    let console_nb = console_nb.with_max_level(Level::WARN);

    let all_files = debug_file_nb.and(warn_file_nb).and(console_nb);

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .with_writer(all_files)
        .init();

    vec![guard_1, guard_2, guard_3]
}
