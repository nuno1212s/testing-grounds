use std::sync::Arc;

use dashmap::DashMap;
use getset::Getters;
use semaphores::RawSemaphore;

use atlas_comm_mio::ByteStubType;
use atlas_common::collections::HashMap;
use atlas_common::node_id::NodeId;
use atlas_communication::byte_stub::{ByteNetworkStub, NodeIncomingStub, NodeStubController};
use atlas_communication::message::WireMessage;
use atlas_communication::reconfiguration::NetworkInformationProvider;
use atlas_metrics::metrics::metric_increment;

use crate::config::TestConfiguration;
use crate::metric::{RQS_RECEIVED_ID, RQS_RECEIVED_IND_ID};

#[derive(Clone, Getters)]
pub(super) struct MockStubController {
    #[get = "pub"]
    concurrency_control: Option<Arc<HashMap<NodeId, Arc<RawSemaphore>>>>,
    test_configuration: TestConfiguration,
    stubs: Arc<DashMap<NodeId, NodeConn>>,
}

pub(super) struct NodeConn {
    byte_stub: Option<ByteStubType>,
    incoming_stub: IncomingStub,
}

#[derive(Clone)]
pub(super) struct IncomingStub {
    concurrency_control: Option<Arc<RawSemaphore>>,
    correlation_node_id: Arc<str>,
}

impl NodeStubController<ByteStubType, IncomingStub> for MockStubController {
    fn has_stub_for(&self, node: &NodeId) -> bool {
        self.stubs.contains_key(node)
    }

    fn generate_stub_for(
        &self,
        node: NodeId,
        byte_stub: ByteStubType,
    ) -> atlas_common::error::Result<IncomingStub>
    where
        ByteStubType: ByteNetworkStub,
        IncomingStub: NodeIncomingStub,
    {
        println!("Generating stub for node {}", node.0);
        let correlation = Arc::from(format!("{}", node.0));

        Ok(self
            .stubs
            .entry(node)
            .or_insert_with(|| NodeConn {
                byte_stub: Some(byte_stub),
                incoming_stub: IncomingStub {
                    concurrency_control: self.concurrency_control.as_ref().and_then(|cc| cc.get(&node)).cloned(),
                    correlation_node_id: correlation,
                },
            })
            .incoming_stub
            .clone())
    }

    fn get_stub_for(&self, node: &NodeId) -> Option<IncomingStub>
    where
        IncomingStub: NodeIncomingStub,
    {
        self.stubs.get(node).map(|stub| stub.incoming_stub.clone())
    }

    fn shutdown_stubs_for(&self, node: &NodeId) {
        self.stubs.remove(node);
    }
}

impl MockStubController {
    pub(super) fn new(
        test_configuration: TestConfiguration,
        node_id: NodeId,
        concurrency_control: Option<Arc<HashMap<NodeId, Arc<RawSemaphore>>>>,
    ) -> Self {
        let stubs: Arc<DashMap<NodeId, NodeConn>> = Arc::new(Default::default());

        stubs.insert(
            node_id,
            NodeConn {
                byte_stub: None,
                incoming_stub: IncomingStub {
                    concurrency_control: concurrency_control.as_ref().and_then(|map| map.get(&node_id)).cloned(),
                    correlation_node_id: Arc::from(format!("{}", node_id.0)),
                },
            },
        );

        Self {
            concurrency_control,
            test_configuration,
            stubs,
        }
    }

    pub(super) fn get_output_stub_for(&self, node: NodeId) -> Option<ByteStubType> {
        self.stubs
            .get(&node)
            .and_then(|conn| conn.value().byte_stub.clone())
    }
}

impl NodeIncomingStub for IncomingStub {
    fn handle_message<NI>(
        &self,
        _network_info: &Arc<NI>,
        _message: WireMessage,
    ) -> atlas_common::error::Result<()>
    where
        NI: NetworkInformationProvider + 'static,
    {
        let concurrency_control = self.concurrency_control.clone();
        
        atlas_common::threadpool::execute(|| {

            if let Some(concurrency_control) = concurrency_control {
                concurrency_control.release();
            };
            
            metric_increment(RQS_RECEIVED_ID, None);
            //metric_correlation_counter_increment(RQS_RECEIVED_ID, self.correlation_node_id.clone(), None);
        });
        
        Ok(())
    }
}
