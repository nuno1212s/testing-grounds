use std::sync::Arc;
use std::time::{Duration, Instant};

use config::File;
use config::FileFormat::Toml;
use semaphores::RawSemaphore;
use tracing::{info, trace, warn};

use atlas_client::client;
use atlas_client::client::ordered_client::Ordered;
use atlas_client::client::unordered_client::UnorderedClientMode;
use atlas_client::client::ClientConfig;
use atlas_client::concurrent_client::ConcurrentClient;
use atlas_comm_mio::config::MIOConfig;
use atlas_common::async_runtime;
use atlas_common::crypto::signature::KeyPair;
use atlas_common::node_id::{NodeId, NodeType};
use atlas_common::peer_addr::PeerAddr;
use atlas_default_configs::crypto::{FlattenedPathConstructor};
use atlas_default_configs::{get_network_configurations, get_reconfig_config};
use atlas_metrics::{with_metric_level, with_metrics, InfluxDBArgs, MetricLevel};
use atlas_reconfiguration::config::ReconfigurableNetworkConfig;

use crate::common::{generate_log, ClientNode, ReconfProtocol, SMRClient, BFT};
use crate::config::benchmark_configs::{
    read_benchmark_config, read_client_config, BenchmarkConfig,
};
use crate::serialize::{MicrobenchmarkData, Request, REQUEST, VERBOSE};

pub(super) fn setup_metrics(influx_db_args: InfluxDBArgs) {
    atlas_metrics::initialize_metrics(
        vec![
            with_metrics(atlas_communication::metric::metrics()),
            with_metrics(atlas_core::metric::metrics()),
            with_metrics(atlas_comm_mio::metrics::metrics()),
            with_metrics(atlas_client::metric::metrics()),
            with_metric_level(MetricLevel::Info),
        ],
        influx_db_args,
    );
}

pub(super) fn client_main() {
    let benchmark = read_benchmark_config().expect("Failed to load benchmark config");

    let client_config = read_client_config().expect("Failed to load client config");

    if client_config.clients_to_run() > 1 {
        multi_client_main(benchmark, client_config.clients_to_run());
    } else {
        setup_and_run_client(benchmark);
    }
}

fn build_reconfigurable_network(
    index: u16,
    node_id: NodeId,
    node_type: NodeType,
    base: ReconfigurableNetworkConfig,
) -> ReconfigurableNetworkConfig {
    let mut network = base;

    network.node_id = node_id;
    network.key_pair = Arc::new(KeyPair::generate_key_pair().unwrap());

    let current_addr = network.our_address.clone();
    let mut current_socket = *current_addr.socket();

    let current_port = current_addr.socket().port();

    current_socket.set_port(current_port + index);

    network.our_address = PeerAddr::new(current_socket, format!("{:?}-{}", node_type, node_id.0));

    network
}

fn generate_network_config(index: u16, node_id: NodeId, network: MIOConfig) -> MIOConfig {
    let mut tcp_configs = network.tcp_configs;

    tcp_configs.network_config = atlas_default_configs::get_tls_config(node_id);

    tcp_configs.bind_addrs = tcp_configs.bind_addrs.map(|addr| {
        let sockets = addr
            .into_iter()
            .map(|mut socket| {
                let port = socket.port();
                socket.set_port(port + index);

                socket
            })
            .collect();

        sockets
    });

    MIOConfig {
        epoll_worker_count: network.epoll_worker_count,
        tcp_configs,
    }
}

pub(super) fn multi_client_main(benchmark: BenchmarkConfig, client_count: u16) {
    let reconfig_config = get_reconfig_config::<FlattenedPathConstructor>(Some(
        format!("config/{}/nodes.toml", 1000).as_str(),
    ))
    .unwrap();

    let node_id = reconfig_config.node_id;

    let influx = atlas_default_configs::influx_db_settings::read_influx_db_config(
        File::new("config/influx_db.toml", Toml),
        Some(node_id),
    )
    .unwrap();

    setup_metrics(influx.into());

    let _log_guard = generate_log(node_id.0);

    let (network_conf, _pool_config) = get_network_configurations(node_id).unwrap();

    let mut handles = Vec::new();

    for i in 0..client_count {
        let benchmark_config = benchmark.clone();
        let reconfig_config = reconfig_config.clone();
        let network_config = network_conf.clone();

        let join_handle = std::thread::spawn(move || {
            let node_id = NodeId(node_id.0 + i as u32);

            setup_run_small_client(
                i,
                node_id,
                benchmark_config,
                reconfig_config,
                network_config,
            );
        });

        handles.push(join_handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}

fn setup_run_small_client(
    index: u16,
    node_id: NodeId,
    benchmark_config: BenchmarkConfig,
    base_reconfigurable_network: ReconfigurableNetworkConfig,
    mio_config: MIOConfig,
) {
    let reconfigurable_network = build_reconfigurable_network(
        index,
        node_id,
        NodeType::Client,
        base_reconfigurable_network,
    );

    println!("Reconfigurable network: {reconfigurable_network:?}");

    let network = generate_network_config(index, node_id, mio_config);

    println!("Network: {network:?}");

    let client_cfg = ClientConfig {
        unordered_rq_mode: UnorderedClientMode::BFT,
        node: network,
        reconfiguration: reconfigurable_network,
    };

    let client = async_runtime::block_on(client::bootstrap_client::<
        ReconfProtocol,
        MicrobenchmarkData,
        ClientNode,
        BFT,
    >(node_id, client_cfg))
    .unwrap();

    info!("Client {:?} initialized!", node_id);

    run_client(client, &benchmark_config);
}

fn setup_and_run_client(benchmark_config: BenchmarkConfig) {
    let reconfig_config = get_reconfig_config::<FlattenedPathConstructor>(Some(
        format!("config/{}/nodes.toml", 1000).as_str(),
    ))
    .unwrap();

    let node_id = reconfig_config.node_id;

    let influx = atlas_default_configs::influx_db_settings::read_influx_db_config(
        File::new("config/influx_db.toml", Toml),
        Some(node_id),
    )
    .unwrap();

    setup_metrics(influx.into());

    let _log_guard = generate_log(node_id.0);

    let (network_conf, _pool_config) = get_network_configurations(node_id).unwrap();

    let client_cfg = ClientConfig {
        unordered_rq_mode: UnorderedClientMode::BFT,
        node: network_conf,
        reconfiguration: reconfig_config,
    };

    let client = async_runtime::block_on(client::bootstrap_client::<
        ReconfProtocol,
        MicrobenchmarkData,
        ClientNode,
        BFT,
    >(node_id, client_cfg))
    .unwrap();

    info!("Client initialized!");

    run_client(client, &benchmark_config);
}

fn run_client(client: SMRClient, benchmark_config: &BenchmarkConfig) {
    let concurrent_rqs = benchmark_config.concurrent_rqs();

    let id = u32::from(client.id());

    info!("Warm up... {id:?}");

    let concurrent_client = ConcurrentClient::from_client(client, concurrent_rqs).unwrap();

    let semaphore = Arc::new(RawSemaphore::new(concurrent_rqs));

    let iterator = 0..(benchmark_config.ops_number() / 2);

    let mut ramp_up = 1000;

    let rq_sleep = Duration::from_millis(benchmark_config.request_sleep_millis() as u64);

    let sem_clone = semaphore.clone();

    let imm_callback = Arc::new(move |_reply| {
        //Release another request for this client
        sem_clone.release();
    });

    for req in iterator {
        //Only allow concurrent_rqs per client at the network
        semaphore.acquire();

        if *VERBOSE {
            trace!("{:?} // Sending req {}...", concurrent_client.id(), req);
        }

        concurrent_client
            .update_imm_callback::<Ordered>(
                Request::new(Arc::clone(&*REQUEST)),
                // I need to replace this
                imm_callback.clone(),
            )
            .unwrap();

        if rq_sleep != Duration::ZERO {
            std::thread::sleep(rq_sleep);
        } else if ramp_up > 0 {
            let to_sleep = fastrand::u32(ramp_up / 2..ramp_up);
            std::thread::sleep(Duration::from_millis(to_sleep as u64));

            ramp_up -= 100;
        }
    }

    info!(
        "Executing experiment for {} ops",
        benchmark_config.ops_number() / 2
    );

    let iterator = 0..(benchmark_config.ops_number() / 2);

    //let mut st = BenchmarkHelper::new(client.id(), MicrobenchmarkData::OPS_NUMBER / 2);

    let start = Instant::now();

    for req in iterator {
        semaphore.acquire();

        if *VERBOSE {
            trace!("Sending req {}...", req);
        }

        concurrent_client
            .update_imm_callback::<Ordered>(
                Request::new(Arc::clone(&*REQUEST)),
                // I need to replace this
                imm_callback.clone(),
            )
            .unwrap();

        if rq_sleep != Duration::ZERO {
            std::thread::sleep(rq_sleep);
        }
    }

    //Wait for all requests to finish
    for _ in 0..concurrent_rqs {
        semaphore.acquire();
    }

    let time_passed = start.elapsed();

    let ops_done = benchmark_config.ops_number() / 2;

    info!("{:?} // Done.", concurrent_client.id());

    warn!(
        "{:?} // Test done in {:?}. ({} ops/s)",
        concurrent_client.id(),
        time_passed,
        (ops_done * 1_000_000) / time_passed.as_micros() as usize
    );
}
