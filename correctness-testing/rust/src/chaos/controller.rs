//! The byte-network controller — the type that plugs into the `atlas-smr-core`
//! networking stack in place of `atlas_comm_mio::MIOTCPNode`.
//!
//! Implements `ByteNetworkController` + `ByteNetworkControllerInit`. On init it builds a
//! `NodeEndpoint` (which owns this node's `PeerConnectionManager` and network info) and
//! registers it, type-erased, into the shared `ChaosMesh`. All actual byte movement is
//! brokered by the mesh; this type just holds the connection controller and the endpoint
//! registration.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

use atlas_common::node_id::NodeId;
use atlas_communication::byte_stub::{
    ByteNetworkController, ByteNetworkControllerInit, NodeIncomingStub, NodeStubController,
};
use atlas_communication::reconfiguration::NetworkInformationProvider;

use crate::chaos::conn_controller::ChaosConnectionController;
use crate::chaos::mesh::{ChaosMesh, NodeDelivery};
use crate::chaos::stub::ChaosByteStub;

/// The `ByteNetworkController::Config` supplied via `ReplicaConfig.node` /
/// `ClientConfig.node`. Carries the shared mesh; the node id is taken from network info.
#[derive(Clone)]
pub struct ChaosConfig {
    pub mesh: Arc<ChaosMesh>,
}

/// Concrete per-node delivery endpoint. Holds the node's real `PeerConnectionManager`
/// (`NSC`) and its network info, and lazily generates one incoming stub per peer.
pub struct NodeEndpoint<NI, IS, NSC> {
    node_id: NodeId,
    network_info: Arc<NI>,
    stub_controller: NSC,
    mesh: Arc<ChaosMesh>,
    inbound: Mutex<BTreeMap<NodeId, IS>>,
}

impl<NI, IS, NSC> NodeEndpoint<NI, IS, NSC>
where
    NI: NetworkInformationProvider + 'static,
    IS: NodeIncomingStub + 'static,
    NSC: NodeStubController<ChaosByteStub, IS> + 'static,
{
    /// Get (or generate exactly once) the incoming stub for messages from `from`.
    /// Generating it also registers, inside `stub_controller`, the paired outbound stub
    /// `self -> from`, which is what enables this node to send to `from` afterwards.
    fn get_or_make_inbound(&self, from: NodeId) -> IS {
        let mut guard = self.inbound.lock().unwrap();
        if let Some(is) = guard.get(&from) {
            return is.clone();
        }
        let out_stub = ChaosByteStub::new(self.node_id, from, self.mesh.clone());
        let is = self
            .stub_controller
            .generate_stub_for(from, out_stub)
            .expect("chaos: generate_stub_for failed");
        guard.insert(from, is.clone());
        is
    }
}

impl<NI, IS, NSC> NodeDelivery for NodeEndpoint<NI, IS, NSC>
where
    NI: NetworkInformationProvider + 'static,
    IS: NodeIncomingStub + 'static,
    NSC: NodeStubController<ChaosByteStub, IS> + 'static,
{
    fn deliver(&self, from: NodeId, msg: atlas_communication::message::WireMessage) {
        let is = self.get_or_make_inbound(from);
        if let Err(e) = is.handle_message(&self.network_info, msg) {
            tracing::warn!(
                "chaos: node {:?} failed to handle message from {:?}: {}",
                self.node_id,
                from,
                e
            );
        }
    }

    fn ensure_inbound(&self, from: NodeId) {
        let _ = self.get_or_make_inbound(from);
    }
}

/// The controller. Generic over the same params as `MIOTCPNode<NI, IS, NSC>`.
pub struct ChaosByteController<NI, IS, NSC> {
    connections: Arc<ChaosConnectionController>,
    _p: PhantomData<fn() -> (NI, IS, NSC)>,
}

// Hand-written Clone: the phantom is `fn() -> _` so the params need no `Clone` bound
// (a derived `Clone` would wrongly require `NI/IS/NSC: Clone`, which the
// `ByteNetworkController: Clone` supertrait then can't satisfy).
impl<NI, IS, NSC> Clone for ChaosByteController<NI, IS, NSC> {
    fn clone(&self) -> Self {
        ChaosByteController {
            connections: self.connections.clone(),
            _p: PhantomData,
        }
    }
}

impl<NI, IS, NSC> ByteNetworkController for ChaosByteController<NI, IS, NSC>
where
    NI: Send + Sync + 'static,
    IS: Send + Sync + 'static,
    NSC: Send + Sync + 'static,
{
    type Config = ChaosConfig;
    type ConnectionController = ChaosConnectionController;

    fn connection_controller(&self) -> &Arc<Self::ConnectionController> {
        &self.connections
    }
}

impl<NI, IS, NSC> ByteNetworkControllerInit<NI, NSC, ChaosByteStub, IS>
    for ChaosByteController<NI, IS, NSC>
where
    NI: NetworkInformationProvider + 'static,
    IS: NodeIncomingStub + 'static,
    NSC: NodeStubController<ChaosByteStub, IS> + 'static,
{
    type Error = Infallible;

    fn initialize_controller(
        network_info: Arc<NI>,
        config: Self::Config,
        stub_controllers: NSC,
    ) -> Result<Self, Self::Error> {
        let node_id = network_info.own_node_info().node_id();
        let mesh = config.mesh;

        let connections = Arc::new(ChaosConnectionController::new(node_id, mesh.clone()));

        let endpoint: Arc<dyn NodeDelivery> = Arc::new(NodeEndpoint {
            node_id,
            network_info,
            stub_controller: stub_controllers,
            mesh: mesh.clone(),
            inbound: Mutex::new(BTreeMap::new()),
        });

        mesh.register_endpoint(node_id, endpoint);

        Ok(ChaosByteController {
            connections,
            _p: PhantomData,
        })
    }
}
