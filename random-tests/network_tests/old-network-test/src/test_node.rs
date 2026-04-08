use crate::config::TestConfiguration;
use crate::metric::{RQS_MADE_ID, RQS_MADE_IND_ID};
use crate::network_info::NetworkInfo;
use atlas_common::channel::OneShotRx;
use atlas_common::crypto::hash::Digest;
use atlas_common::node_id::NodeId;
use atlas_communication::message::{WireMessage};
use atlas_metrics::metrics::{metric_correlation_counter_increment_arc, metric_increment};
use semaphores::RawSemaphore;
use std::sync::{Arc, Barrier};
use atlas_communication::config::MioConfig;
use atlas_communication::{FullNetworkNode, NetworkNode, NodeConnections};
use atlas_communication::mio_tcp::MIOTcpNode;
use atlas_communication::reconfiguration_node::NetworkInformationProvider;
use atlas_communication::serialize::Buf;
use crate::mocks::{ProtocolMessage, ReconfigurableMessage};

pub(super) fn init_node(
    node_info: Arc<NetworkInfo>,
    config: MioConfig,
) -> atlas_common::error::Result<MIOTcpNode<NetworkInfo, ReconfigurableMessage, ProtocolMessage>> {
    let node_id = node_info.get_own_id();

    atlas_common::async_runtime::block_on(MIOTcpNode::bootstrap(
        node_id,
        node_info,
        config
    ))
}

fn connect_to_node(
    node: &MIOTcpNode<NetworkInfo, ReconfigurableMessage, ProtocolMessage>,
    id: NodeId,
) -> atlas_common::error::Result<()> {

    let to_wait = node.node_connections().connect_to_node(id);

    to_wait
        .into_iter()
        .flat_map(OneShotRx::recv)
        .collect::<atlas_common::error::Result<()>>()
}

pub(super) fn run_test_node(
    node: MIOTcpNode<NetworkInfo, ReconfigurableMessage, ProtocolMessage>,
    id: NodeId,
    test_config: TestConfiguration,
    message: Buf,
    barrier: Arc<Barrier>,
) -> atlas_common::error::Result<()> {
    println!("Node {} started", id.0);

    for node_id in 0..test_config.node_count() as u32 {
        if id.0 == node_id {
            continue;
        }

        println!("Node {} connecting to node {}", id.0, node_id);
        connect_to_node(&node, NodeId(node_id))?;
        println!("Node {} connected to node {}", id.0, node_id);
    }

    /*let our_concurrency_control = node
        .stub_controller()
        .concurrency_control()
        .as_ref()
        .and_then(|cc| cc.get(&id))
        .cloned();*/

    barrier.wait();

    println!("Node {} sending messages", id.0);

    for op_ind in 0..test_config.messages_to_send() {
        let target_node = NodeId((op_ind % test_config.node_count()) as u32);

        if let Some(target_conn) = node.get_direct_stub_to(&target_node) {

            let message = WireMessage::new(
                id,
                target_node,
                message.clone(),
                0,
                Some(Digest::blank()),
                None,
            );

            target_conn.peer_message(message, None)
                .expect("Failed to send message");

            metric_increment(RQS_MADE_ID, None);
        }

    }

    println!("Node {} finished sending messages", id.0);

    Ok(())
}
