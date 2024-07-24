use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use getset::Getters;

use atlas_common::collections::HashMap;
use atlas_common::crypto::signature::{KeyPair, PublicKey};
use atlas_common::node_id::{NodeId, NodeType};
use atlas_common::peer_addr::PeerAddr;
use atlas_communication::reconfiguration::{NetworkInformationProvider, NodeInfo};

use crate::config::TestConfiguration;

#[derive(Getters)]
pub(super) struct NetworkInfo {
    own_node: NodeId,
    #[get = "pub"]
    correlation_node_id: Arc<str>,
    node_info: HashMap<NodeId, NodeInfo>,
    key_pair: Arc<KeyPair>,
}

impl NetworkInformationProvider for NetworkInfo {
    fn own_node_info(&self) -> &NodeInfo {
        self.node_info.get(&self.own_node).unwrap()
    }

    fn get_key_pair(&self) -> &Arc<KeyPair> {
        &self.key_pair
    }

    fn get_node_info(&self, node: &NodeId) -> Option<NodeInfo> {
        self.node_info.get(node).cloned()
    }
}

impl NetworkInfo {
    pub fn init_info_from_test_config(test: TestConfiguration) -> HashMap<usize, Self> {
        let mut nodes = HashMap::default();

        let mut node_infos = HashMap::default();

        let mut key_pairs = HashMap::default();

        for node in 0..test.node_count() {
            let key_pair = KeyPair::generate_key_pair().unwrap();

            let pk = key_pair.public_key().into();

            key_pairs.insert(node, Arc::new(key_pair));

            let ip = Ipv4Addr::new(127, 0, 0, 1);

            let addr = IpAddr::from(ip);

            let hostname = format!("node-{}", node);

            let port = test.base_port() + node as u16;

            let node_id = NodeId(node as u32);

            let node_info = NodeInfo::new(
                node_id,
                NodeType::Replica,
                pk,
                PeerAddr::new(SocketAddr::new(addr, port), hostname),
            );

            node_infos.insert(node_id, node_info);
        }

        for node in 0..test.node_count() {
            let network_info = Self {
                own_node: NodeId(node as u32),
                correlation_node_id: Arc::from(format!("{}", node)),
                node_info: node_infos.clone(),
                key_pair: key_pairs.get(&node).unwrap().clone(),
            };

            nodes.insert(node, network_info);
        }

        nodes
    }
}
