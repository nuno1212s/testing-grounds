//! The concrete generic composition of the whole SMR stack, for febft.
//!
//! This is a near-verbatim copy of the proven febft stack in
//! `testing-grounds/crud_perf/crud_perf_exec/src/common.rs`, with exactly two
//! substitutions:
//!   * `ByteNetworkLayer` / `CLIByteNetworkLayer`: `MIOTCPNode` → `ChaosByteController`
//!   * `ByteStubType`: `atlas_comm_mio::ByteStubType` → `crate::chaos::ChaosByteStub`
//! and the application types swapped for `KvEcho`/`KvEchoData`/`KvState`.
//!
//! Everything else (protocol/log/view-transfer/state-transfer/persistent-log types) is
//! generic over the byte layer and copied unchanged — that is the whole point of the
//! two-swap-point transport design.
#![allow(dead_code)]

use atlas_client::client::Client;
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

use crate::apps::kv_echo::{KvEcho, KvEchoData, KvState};
use crate::chaos::{ChaosByteController, ChaosByteStub};

// ── Application types ────────────────────────────────────────────────────────────────
pub type AppData = KvEchoData;
pub type State = KvState;
pub type App = KvEcho;

// ── Message-serialization types ──────────────────────────────────────────────────────
pub type ReconfigurationMessage = ReconfData;

pub type OrderProtocolMessage = PBFTConsensus<SMRReq<AppData>>;
pub type DecLogMsg = LogSerialization<SMRReq<AppData>, OrderProtocolMessage, OrderProtocolMessage>;
pub type LogTransferMessage =
    LTMsg<SMRReq<AppData>, OrderProtocolMessage, OrderProtocolMessage, DecLogMsg>;
pub type ViewTransferMessage = ViewTransfer<OrderProtocolMessage>;

pub type StateTransferMessage = CSTMsg<State>;
pub type SerStateTransferMessage = StateSys<StateTransferMessage>;

pub type ProtocolDataType =
    Service<AppData, OrderProtocolMessage, LogTransferMessage, ViewTransferMessage>;

// ── Byte layer (THE swap point) ──────────────────────────────────────────────────────
pub type ByteStubType = ChaosByteStub;

// ── Replica networking layer ─────────────────────────────────────────────────────────
pub type IncomingStub = NodeInputStub<
    ReconfigurationMessage,
    ProtocolDataType,
    SerStateTransferMessage,
    SMRSysMsg<AppData>,
>;
pub type StubController = NodeStubController<
    NetworkInfo,
    ByteStubType,
    ReconfigurationMessage,
    ProtocolDataType,
    SerStateTransferMessage,
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

// ── Client networking layer ──────────────────────────────────────────────────────────
pub type CLIIncomingStub =
    NodeInputStub<ReconfigurationMessage, NoProtocol, NoProtocol, SMRSysMsg<AppData>>;
pub type CLIStubController = NodeStubController<
    NetworkInfo,
    ByteStubType,
    ReconfigurationMessage,
    NoProtocol,
    NoProtocol,
    SMRSysMsg<AppData>,
>;

pub type CLIByteNetworkLayer = ChaosByteController<NetworkInfo, CLIIncomingStub, CLIStubController>;

pub type ClientNode = CLINodeWrapper<
    ByteStubType,
    CLIByteNetworkLayer,
    NetworkInfo,
    ReconfigurationMessage,
    AppData,
>;

pub type ClientNetwork =
    <ClientNode as SMRClientNetworkNode<NetworkInfo, ReconfigurationMessage, AppData>>::AppNode;

// ── Persistent log + protocol stack ──────────────────────────────────────────────────
pub type Logging = MonStatePersistentLog<
    State,
    AppData,
    OrderProtocolMessage,
    OrderProtocolMessage,
    DecLogMsg,
    StateTransferMessage,
>;

pub type ReconfProtocol = ReconfigurableNodeProtocolHandle;
pub type OrderProtocol =
    PBFTOrderProtocol<SMRReq<AppData>, RequestPreProcessor<SMRReq<AppData>>, ProtocolNetwork>;
pub type Executor = MonolithicPreemptiveExecutor;
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

pub type SMRClient = Client<ReconfProtocol, AppData, ClientNetwork>;

/// BFT tolerance: n = 3f+1, quorum = 2f+1.
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
