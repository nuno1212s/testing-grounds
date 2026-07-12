//! The concrete SMR generic composition, split into a protocol-agnostic core (this
//! module) plus one module per ordering protocol (`febft`, `hotstuff`).
//!
//! The client side carries no ordering-protocol type parameter, so it — along with the
//! application, state-transfer message type, and byte layer — is shared. Only the replica
//! composition (ordering protocol, decision log, replica config) differs per protocol.
#![allow(dead_code)]

pub mod chained;
pub mod febft;
pub mod hotstuff;

use atlas_client::client::Client;
use atlas_communication::{NodeInputStub, NodeStubController};
use atlas_core::ordering_protocol::OrderProtocolTolerance;
use atlas_core::serialize::NoProtocol;
use atlas_reconfiguration::ReconfigurableNodeProtocolHandle;
use atlas_reconfiguration::message::ReconfData;
use atlas_reconfiguration::network_reconfig::NetworkInfo;
use atlas_smr_core::networking::client::{CLINodeWrapper, SMRClientNetworkNode};
use atlas_smr_core::serialize::{SMRSysMsg, StateSys};
use febft_state_transfer::message::serialize::CSTMsg;

use crate::apps::kv_echo::{KvEcho, KvEchoData, KvState};
use crate::chaos::{ChaosByteController, ChaosByteStub};

// ── Application + shared message types ───────────────────────────────────────────────
pub type AppData = KvEchoData;
pub type State = KvState;
pub type App = KvEcho;

pub type ReconfigurationMessage = ReconfData;
pub type ReconfProtocol = ReconfigurableNodeProtocolHandle;

// State transfer reuses febft-state-transfer's `CSTMsg` for ALL protocols (precedented
// by microbenchmark-hotstuff), so the state-transfer message type is shared.
pub type StateTransferMessage = CSTMsg<State>;
pub type SerStateTransferMessage = StateSys<StateTransferMessage>;

// The byte-layer stub (the chaos transport) is shared.
pub type ByteStubType = ChaosByteStub;

// ── Client networking layer (protocol-agnostic: NoProtocol for O/S) ──────────────────
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
pub type SMRClient = Client<ReconfProtocol, AppData, ClientNetwork>;

/// BFT tolerance: n = 3f+1, quorum = 2f+1. Shared across protocols.
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
