use crate::config::TestConfiguration;
use crate::metric::{RQS_MADE_ID, RQS_MADE_IND_ID};
use crate::mocks::{IncomingStub, MockStubController};
use crate::network_info::NetworkInfo;
use atlas_comm_mio::config::MIOConfig;
use atlas_comm_mio::MIOTCPNode;
use atlas_common::channel::OneShotRx;
use atlas_common::collections::HashMap;
use atlas_common::crypto::hash::Digest;
use atlas_common::node_id::NodeId;
use atlas_communication::byte_stub::connections::NetworkConnectionController;
use atlas_communication::byte_stub::{
    ByteNetworkController, ByteNetworkControllerInit, ByteNetworkStub,
};
use atlas_communication::lookup_table::MessageModule;
use atlas_communication::message::{Buf, WireMessage};
use atlas_communication::reconfiguration::NetworkInformationProvider;
use atlas_metrics::metrics::{metric_correlation_counter_increment_arc, metric_increment};
use semaphores::RawSemaphore;
use std::sync::{Arc, Barrier};

pub(super) fn init_node(
    node_info: Arc<NetworkInfo>,
    config: MIOConfig,
    test_configuration: TestConfiguration,
    concurrency_control: Option<Arc<HashMap<NodeId, Arc<RawSemaphore>>>>,
) -> atlas_common::error::Result<MIOTCPNode<NetworkInfo, IncomingStub, MockStubController>> {
    let node_id = node_info.own_node_info().node_id();

    MIOTCPNode::initialize_controller(
        node_info,
        config,
        MockStubController::new(test_configuration, node_id, concurrency_control),
    )
}

fn connect_to_node(
    node: &MIOTCPNode<NetworkInfo, IncomingStub, MockStubController>,
    id: NodeId,
) -> atlas_common::error::Result<()> {
    let to_wait = node.connection_controller().connect_to_node(id)?;

    to_wait
        .into_iter()
        .flat_map(OneShotRx::recv)
        .collect::<atlas_common::error::Result<()>>()
}

pub(super) fn run_test_node(
    node: MIOTCPNode<NetworkInfo, IncomingStub, MockStubController>,
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

    let correlation: Arc<str> = Arc::from(format!("{}", id.0));

    let our_concurrency_control = node
        .stub_controller()
        .concurrency_control()
        .as_ref()
        .and_then(|cc| cc.get(&id))
        .cloned();

    println!("Node {} sending messages", id.0);

    barrier.wait();

    for op_ind in 0..test_config.messages_to_send() {
        let target_node = NodeId((op_ind % test_config.node_count()) as u32);

        if let Some(output_stub) = node.stub_controller().get_output_stub_for(target_node) {
            our_concurrency_control.as_ref().map(|sem| sem.acquire());

            let message = WireMessage::new(
                id,
                target_node,
                MessageModule::Application,
                message.clone(),
                0,
                Some(Digest::blank()),
                None,
            );

            output_stub
                .dispatch_blocking(message)
                .expect("Failed to dispatch message");

            metric_increment(RQS_MADE_IND_ID, None);
            //metric_correlation_counter_increment_arc(RQS_MADE_ID, correlation.clone(), None);
        }
    }

    println!("Node {} finished sending messages", id.0);

    Ok(())
}
