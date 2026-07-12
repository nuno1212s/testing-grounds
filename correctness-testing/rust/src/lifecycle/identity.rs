//! In-process PKI synthesis.
//!
//! Generates the cluster's Ed25519 identities and hands out the per-node
//! `ReconfigurableNetworkConfig` that `Replica::bootstrap` turns into the real
//! `atlas_reconfiguration::network_reconfig::NetworkInfo`.
//!
//! F6: unlike the workspace mocks (`MockNetworkInfoFactory`), which seed *every* node
//! from `[0u8; 32]` and thus give all nodes an identical keypair, we derive a DISTINCT
//! keypair per node (`seed = f(node_id)`). Identical keys would make signature-based
//! equivocation checks meaningless — and distinct keys are the realistic case anyway.

use std::collections::BTreeMap;
use std::sync::Arc;

use atlas_common::crypto::signature::{KeyPair, PublicKey};
use atlas_common::node_id::{NodeId, NodeType};
use atlas_common::peer_addr::PeerAddr;
use atlas_communication::reconfiguration::NodeInfo;
use atlas_reconfiguration::config::ReconfigurableNetworkConfig;

/// Base port for the synthesized `PeerAddr`s. These addresses are never bound (the
/// chaos transport uses no sockets), but the reconfiguration layer stores them.
const BASE_PORT: u16 = 10_000;

/// Derive a distinct 32-byte Ed25519 seed for a node (F6).
fn seed_for(node_id: NodeId) -> [u8; 32] {
    let mut seed = [0u8; 32];
    seed[..4].copy_from_slice(&node_id.0.to_le_bytes());
    // Spread the id a little so small ids don't share long common prefixes.
    seed[4..8].copy_from_slice(&(node_id.0.wrapping_mul(0x9E37_79B9)).to_le_bytes());
    seed
}

fn addr_for(node_id: NodeId) -> PeerAddr {
    let port = BASE_PORT.wrapping_add((node_id.0 & 0xFFFF) as u16);
    PeerAddr::new(
        format!("127.0.0.1:{port}")
            .parse()
            .expect("valid socket addr"),
        format!("node-{}", node_id.0),
    )
}

/// The synthesized identities for a cluster: distinct keypairs for every replica.
pub struct ClusterIdentity {
    replicas: Vec<NodeId>,
    keys: BTreeMap<NodeId, Arc<KeyPair>>,
    /// Public `NodeInfo` for every replica — this is the bootstrap-node list.
    replica_infos: Vec<NodeInfo>,
}

impl ClusterIdentity {
    /// Generate distinct identities for replica ids `0..n`.
    pub fn new(n: usize) -> Self {
        let replicas: Vec<NodeId> = (0..n as u32).map(NodeId::from).collect();

        let mut keys = BTreeMap::new();
        let mut replica_infos = Vec::with_capacity(n);

        for &node_id in &replicas {
            let key = KeyPair::from_bytes(&seed_for(node_id)).expect("keypair from seed");
            let public_key = PublicKey::from(key.public_key());
            replica_infos.push(NodeInfo::new(
                node_id,
                NodeType::Replica,
                public_key,
                addr_for(node_id),
            ));
            keys.insert(node_id, Arc::new(key));
        }

        ClusterIdentity {
            replicas,
            keys,
            replica_infos,
        }
    }

    pub fn replica_ids(&self) -> &[NodeId] {
        &self.replicas
    }

    /// Build the reconfiguration config for a replica node. `known_nodes` is the full
    /// bootstrap set (all replicas, including this one — matching a real `nodes.toml`).
    pub fn replica_reconfig_config(&self, node_id: NodeId) -> ReconfigurableNetworkConfig {
        let key_pair = self
            .keys
            .get(&node_id)
            .expect("replica id has a synthesized key")
            .clone();

        ReconfigurableNetworkConfig {
            node_id,
            node_type: NodeType::Replica,
            key_pair,
            our_address: addr_for(node_id),
            known_nodes: self.replica_infos.clone(),
        }
    }

    /// Build the reconfiguration config for a client node. Clients get a fresh distinct
    /// keypair and know the replica bootstrap set.
    pub fn client_reconfig_config(&self, node_id: NodeId) -> ReconfigurableNetworkConfig {
        let key = KeyPair::from_bytes(&seed_for(node_id)).expect("client keypair from seed");

        ReconfigurableNetworkConfig {
            node_id,
            node_type: NodeType::Client,
            key_pair: Arc::new(key),
            our_address: addr_for(node_id),
            known_nodes: self.replica_infos.clone(),
        }
    }
}
