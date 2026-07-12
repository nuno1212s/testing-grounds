//! The per-directed-edge outbound byte stub.
//!
//! One `ChaosByteStub` represents "how node `from` sends to node `to`". It is what the
//! higher `atlas-communication` layer obtains (via `NodeStubController::generate_stub_for`)
//! and calls `dispatch_blocking` on for every outbound peer message. Every such call is
//! funneled straight into the shared `ChaosMesh`.

use std::sync::Arc;

use atlas_common::node_id::NodeId;
use atlas_communication::byte_stub::{ByteNetworkStub, DispatchError};
use atlas_communication::message::WireMessage;

use crate::chaos::mesh::ChaosMesh;

#[derive(Clone)]
pub struct ChaosByteStub {
    from: NodeId,
    to: NodeId,
    mesh: Arc<ChaosMesh>,
}

impl ChaosByteStub {
    pub fn new(from: NodeId, to: NodeId, mesh: Arc<ChaosMesh>) -> Self {
        ChaosByteStub { from, to, mesh }
    }
}

impl ByteNetworkStub for ChaosByteStub {
    // Reuse atlas-communication's DispatchError (it already implements
    // ByteNetworkDispatchError + std::error::Error, as the trait requires).
    type Error = DispatchError;

    fn dispatch_blocking(&self, message: WireMessage) -> Result<(), Self::Error> {
        self.mesh.dispatch(self.from, self.to, message);
        Ok(())
    }
}
