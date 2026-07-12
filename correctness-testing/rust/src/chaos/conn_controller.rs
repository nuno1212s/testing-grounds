//! The per-node connection controller.
//!
//! Implements `NetworkConnectionController` by delegating to the shared `ChaosMesh`,
//! which is the authoritative owner of connection state. This matters for F2: a
//! `true_crash` severs the node in the mesh, and `has_connection()` — which the
//! reconfiguration protocol genuinely queries
//! (`Atlas-Reconfiguration/src/network_reconfig/mod.rs:556,697,828`) — then correctly
//! reports the crashed node as disconnected.

use std::sync::Arc;

use atlas_common::channel::oneshot::new_oneshot_channel;
use atlas_common::node_id::NodeId;
use atlas_communication::byte_stub::connections::{
    IndividualConnectionResult, NetworkConnectionController,
};
use thiserror::Error;

use crate::chaos::mesh::ChaosMesh;

#[derive(Error, Debug)]
#[error("chaos connection error")]
pub struct ChaosConnError;

pub struct ChaosConnectionController {
    node_id: NodeId,
    mesh: Arc<ChaosMesh>,
}

impl ChaosConnectionController {
    pub fn new(node_id: NodeId, mesh: Arc<ChaosMesh>) -> Self {
        ChaosConnectionController { node_id, mesh }
    }
}

impl NetworkConnectionController for ChaosConnectionController {
    type IndConnError = ChaosConnError;
    type ConnectionError = ChaosConnError;

    fn has_connection(&self, node: &NodeId) -> bool {
        self.mesh.has_connection(self.node_id, *node)
    }

    fn currently_connected_node_count(&self) -> usize {
        self.mesh.connected_nodes(self.node_id).len()
    }

    fn currently_connected_nodes(&self) -> Vec<NodeId> {
        self.mesh.connected_nodes(self.node_id)
    }

    fn connect_to_node(
        self: &Arc<Self>,
        node: NodeId,
    ) -> Result<Vec<IndividualConnectionResult<Self::IndConnError>>, Self::ConnectionError> {
        // In-process connections are instantaneous: wire up the edge in the mesh and
        // hand back an already-resolved success. The reconfiguration protocol blocks on
        // `conn_result.recv()` (network_reconfig/mod.rs:482), so this must be pre-fired.
        self.mesh.connect(self.node_id, node);

        let (tx, rx) = new_oneshot_channel::<Result<(), Self::IndConnError>>();
        let _ = tx.send(Ok(()));
        Ok(vec![rx])
    }

    fn disconnect_from_node(self: &Arc<Self>, node: &NodeId) -> Result<(), Self::ConnectionError> {
        self.mesh.disconnect(self.node_id, *node);
        Ok(())
    }
}
