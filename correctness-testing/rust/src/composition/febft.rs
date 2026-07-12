//! febft (PBFT) replica composition. Everything protocol-specific for febft lives here;
//! shared app/client/state-transfer types come from the parent module.
#![allow(dead_code)]

use atlas_decision_log::Boule;
use atlas_decision_log::serialize::LogSerialization;
use atlas_log_transfer::CollabLogTransfer;
use atlas_log_transfer::messages::serialize::LTMsg;
use atlas_persistent_log::stateful_logs::monolithic_state::MonStatePersistentLog;
use atlas_reconfiguration::network_reconfig::NetworkInfo;
use atlas_smr_core::SMRReq;
use atlas_smr_core::execution::{SMRExecWrapper, TExecutor};
use atlas_smr_core::networking::{ReplicaNodeWrapper, SMRReplicaNetworkNode};
use atlas_smr_core::request_pre_processing::RequestPreProcessor;
use atlas_smr_core::serialize::{SMRSysMsg, Service, StateSys};
use atlas_smr_execution::SingleThreadedMonExecutor;
use atlas_smr_replica::config::{MonolithicStateReplicaConfig, ReplicaConfig};
use atlas_smr_replica::server::monolithic_server::MonReplica;
use atlas_view_transfer::SimpleViewTransferProtocol;
use atlas_view_transfer::message::serialize::ViewTransfer;
use febft_pbft_consensus::bft::PBFTOrderProtocol;
use febft_pbft_consensus::bft::message::serialize::PBFTConsensus;
use febft_state_transfer::CollabStateTransfer;

use atlas_communication::{NodeInputStub, NodeStubController};
use super::{
    App, AppData, ByteStubType, ReconfProtocol, ReconfigurationMessage, State,
    StateTransferMessage,
};
use crate::chaos::ChaosByteController;

pub type OrderProtocolMessage = PBFTConsensus<SMRReq<AppData>>;
pub type DecLogMsg = LogSerialization<SMRReq<AppData>, OrderProtocolMessage, OrderProtocolMessage>;
pub type LogTransferMessage =
    LTMsg<SMRReq<AppData>, OrderProtocolMessage, OrderProtocolMessage, DecLogMsg>;
pub type ViewTransferMessage = ViewTransfer<OrderProtocolMessage>;

pub type ProtocolDataType =
    Service<AppData, OrderProtocolMessage, LogTransferMessage, ViewTransferMessage>;

pub type IncomingStub = NodeInputStub<
    ReconfigurationMessage,
    ProtocolDataType,
    super::SerStateTransferMessage,
    SMRSysMsg<AppData>,
>;
pub type StubController = NodeStubController<
    NetworkInfo,
    ByteStubType,
    ReconfigurationMessage,
    ProtocolDataType,
    super::SerStateTransferMessage,
    SMRSysMsg<AppData>,
>;
pub type ByteNetworkLayer = ChaosByteController<NetworkInfo, IncomingStub, StubController>;

pub type ReplicaNode = ReplicaNodeWrapper<
    ByteStubType,
    ByteNetworkLayer,
    NetworkInfo,
    ReconfigurationMessage,
    AppData,
    OrderProtocolMessage,
    LogTransferMessage,
    ViewTransferMessage,
    StateTransferMessage,
>;

pub type ProtocolNetwork = <ReplicaNode as SMRReplicaNetworkNode<
    NetworkInfo,
    ReconfigurationMessage,
    AppData,
    OrderProtocolMessage,
    LogTransferMessage,
    ViewTransferMessage,
    StateTransferMessage,
>>::ProtocolNode;

pub type StateTransferNetwork = <ReplicaNode as SMRReplicaNetworkNode<
    NetworkInfo,
    ReconfigurationMessage,
    AppData,
    OrderProtocolMessage,
    LogTransferMessage,
    ViewTransferMessage,
    StateTransferMessage,
>>::StateTransferNode;

pub type AppNetwork = <ReplicaNode as SMRReplicaNetworkNode<
    NetworkInfo,
    ReconfigurationMessage,
    AppData,
    OrderProtocolMessage,
    LogTransferMessage,
    ViewTransferMessage,
    StateTransferMessage,
>>::ApplicationNode;

pub type Logging = MonStatePersistentLog<
    State,
    AppData,
    OrderProtocolMessage,
    OrderProtocolMessage,
    DecLogMsg,
    StateTransferMessage,
>;

pub type OrderProtocol =
    PBFTOrderProtocol<SMRReq<AppData>, RequestPreProcessor<SMRReq<AppData>>, ProtocolNetwork>;
pub type Executor = SingleThreadedMonExecutor<AppNetwork>;
pub type ExecutorHandle = SMRExecWrapper<<Executor as TExecutor<App, State>>::ExecutionHandle>;

pub type DecisionLog = Boule<SMRReq<AppData>, OrderProtocol, Logging, ExecutorHandle>;
pub type LogTransferProtocol = CollabLogTransfer<
    SMRReq<AppData>,
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
    AppData,
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
    App,
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
    App,
    OrderProtocol,
    DecisionLog,
    StateTransferProtocol,
    LogTransferProtocol,
    ViewTransferProt,
    ReplicaNode,
    Logging,
>;
