use crate::config::{get_configuration_for_test, TestConfiguration};
use crate::network_info::NetworkInfo;
use crate::test_node::{init_node, run_test_node};
use atlas_common::collections::HashMap;
use atlas_common::node_id::NodeId;
use atlas_common::{init, InitConfig};
use atlas_communication::message::Buf;
use atlas_default_configs::runtime_settings::RunTimeSettings;
use atlas_default_configs::{get_network_configurations, get_runtime_configuration};
use atlas_metrics::with_metrics;
use semaphores::RawSemaphore;
use std::iter;
use std::sync::{Arc, Barrier};

mod config;
mod metric;
mod mocks;
mod network_info;
mod test_node;

fn init_metrics() {
    let metric = atlas_default_configs::get_influx_configuration(Some(NodeId(0)))
        .expect("Failed to get influx configuration");

    atlas_metrics::initialize_metrics(vec![with_metrics(metric::metrics())], metric);
}

fn main() {
    let runtime_config = get_runtime_configuration().expect("Failed to get runtime configuration");

    init_metrics();

    let RunTimeSettings {
        threadpool_threads,
        async_runtime_threads,
    } = runtime_config;

    let conf = InitConfig {
        //If we are the client, we want to have many threads to send stuff to replicas
        threadpool_threads,
        async_threads: async_runtime_threads,
    };

    let _guard = unsafe { init(conf).unwrap() };

    let configuration = get_configuration_for_test();

    let mut network_infos = NetworkInfo::init_info_from_test_config(configuration.clone());

    let concurrency_control = if configuration.concurrent_rqs_per_node() > 0 {
        Some(generate_concurrency_control(&configuration))
    } else {
        None
    };
    

    let bytes = Buf::from(
        iter::repeat_with(rand::random::<u8>)
            .take(1024)
            .collect::<Vec<u8>>(),
    );

    let mut join_handles = Vec::new();

    let barrier = Arc::new(Barrier::new(configuration.node_count()));

    for node in 0..configuration.node_count() {
        let node_id = NodeId(node as u32);
        let (mio_config, _pool) =
            get_network_configurations(node_id).expect("Failed to get network config");

        let network_info = Arc::new(
            network_infos
                .remove(&node)
                .expect("Failed to get network info"),
        );

        let node = init_node(
            network_info,
            mio_config,
            configuration.clone(),
            concurrency_control.clone(),
        )
        .expect("Failed to initialize node");

        let cfg_clone = configuration.clone();
        let bytes = bytes.clone();
        let barrier = Arc::clone(&barrier);

        join_handles.push(std::thread::spawn(move || {
            run_test_node(node, node_id, cfg_clone, bytes, barrier).expect("Failed to run node");
        }))
    }

    for x in join_handles {
        x.join().expect("Failed to join thread");
    }
}

fn generate_concurrency_control(
    test_configuration: &TestConfiguration,
) -> Arc<HashMap<NodeId, Arc<RawSemaphore>>> {
    let mut concurrency_control = HashMap::default();

    for node in 0..test_configuration.node_count() {
        concurrency_control.insert(
            NodeId(node as u32),
            Arc::new(RawSemaphore::new(
                test_configuration.concurrent_rqs_per_node(),
            )),
        );
    }

    Arc::new(concurrency_control)
}
