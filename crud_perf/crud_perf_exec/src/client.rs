use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use config::File;
use config::FileFormat::Toml;
use semaphores::RawSemaphore;
use tracing::{info, warn};

use atlas_client::client;
use atlas_client::client::ClientConfig;
use atlas_client::client::ordered_client::Ordered;
use atlas_client::client::unordered_client::{Unordered, UnorderedClientMode};
use atlas_client::concurrent_client::ConcurrentClient;
use atlas_comm_mio::config::MIOConfig;
use atlas_common::async_runtime;
use atlas_common::crypto::signature::KeyPair;
use atlas_common::node_id::{NodeId, NodeType};
use atlas_common::peer_addr::PeerAddr;
use atlas_default_configs::{get_network_configurations, get_reconfig_config};
use atlas_metrics::metrics::{
    metric_correlation_time_end, metric_correlation_time_start, metric_increment,
};
use atlas_metrics::{InfluxDBArgs, MetricLevel, with_metric_level, with_metrics};
use atlas_reconfiguration::config::ReconfigurableNetworkConfig;

use crate::common::{BFT, ClientNode, ReconfProtocol, SMRClient, generate_log};
use crate::config::benchmark_configs::{
    BenchmarkConfig, read_benchmark_config, read_client_config,
};
use crate::metric::{
    CRUD_CLIENT_LATENCY_ID, CRUD_CLIENT_OPS_DONE_ID, CRUD_LATENCY_DELETE_ID, CRUD_LATENCY_READ_ID,
    CRUD_LATENCY_WRITE_ID,
};
use crate::serialize::{CRUDRequest, CRUDRequestType, MicrobenchmarkData};
use crate::workload::{KeyDistributionKind, WorkloadGenerator, WorkloadType};
use atlas_default_configs::crypto::FlattenedPathConstructor;

pub(super) fn setup_metrics(influx_db_args: InfluxDBArgs) {
    atlas_metrics::initialize_metrics(
        vec![
            with_metrics(atlas_communication::metric::metrics()),
            with_metrics(atlas_core::metric::metrics()),
            with_metrics(atlas_comm_mio::metrics::metrics()),
            with_metrics(atlas_client::metric::metrics()),
            with_metrics(crate::metric::metrics()),
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
    let mut current_socket = current_addr.socket().clone();
    let current_port = current_addr.socket().port();
    current_socket.set_port(current_port + index);
    network.our_address = PeerAddr::new(current_socket, format!("{:?}-{}", node_type, node_id.0));

    network
}

fn generate_network_config(index: u16, node_id: NodeId, network: MIOConfig) -> MIOConfig {
    let mut tcp_configs = network.tcp_configs;
    tcp_configs.network_config = atlas_default_configs::get_tls_config(node_id);
    tcp_configs.bind_addrs = tcp_configs.bind_addrs.map(|addr| {
        addr.into_iter()
            .map(|mut socket| {
                let port = socket.port();
                socket.set_port(port + index);
                socket
            })
            .collect()
    });

    MIOConfig {
        epoll_worker_count: network.epoll_worker_count,
        tcp_configs,
    }
}

pub(super) fn multi_client_main(benchmark: BenchmarkConfig, client_count: u16) {
    let mut reconfig_config =
        get_reconfig_config::<FlattenedPathConstructor>(Some("config/nodes.toml")).unwrap();

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

    let network = generate_network_config(index, node_id, mio_config);

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

    run_client(client, benchmark_config);
}

fn setup_and_run_client(benchmark_config: BenchmarkConfig) {
    let reconfig_config = get_reconfig_config::<FlattenedPathConstructor>(None).unwrap();
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

    run_client(client, benchmark_config);
}

fn run_client(client: SMRClient, benchmark_config: BenchmarkConfig) {
    let concurrent_rqs = benchmark_config.concurrent_rqs();
    let id = u32::from(client.id());
    let force_ordered = benchmark_config.force_ordered();

    let workload_type = WorkloadType::from_str(&benchmark_config.workload_type);
    let key_dist = KeyDistributionKind::from_str(&benchmark_config.key_distribution);
    let key_space = benchmark_config.effective_key_space_size() as u64;

    let mut generator = WorkloadGenerator::new(
        id as u64 * 7919,
        key_space,
        workload_type,
        key_dist,
        benchmark_config.zipf_constant(),
        benchmark_config.expensive_op_sleep_ms() as u64,
        benchmark_config.read_ratio(),
        benchmark_config.create_ratio(),
        benchmark_config.update_ratio(),
        benchmark_config.delete_ratio(),
    );

    let concurrent_client = ConcurrentClient::from_client(client, concurrent_rqs).unwrap();
    let semaphore = Arc::new(RawSemaphore::new(concurrent_rqs));

    // ── Warm-up phase ──
    info!("Warm up ({} ops)...", benchmark_config.ops_number() / 2);

    let warmup_sem = semaphore.clone();
    let warmup_callback = Arc::new(move |_reply| {
        warmup_sem.release();
    });

    for _ in 0..(benchmark_config.ops_number() / 2) {
        semaphore.acquire();
        let op = generator.next_op(force_ordered);
        let request = CRUDRequest::new(op.time_delay, op.request_type);

        if op.is_ordered {
            concurrent_client
                .update_imm_callback::<Ordered>(request, warmup_callback.clone())
                .unwrap();
        } else {
            concurrent_client
                .update_imm_callback::<Unordered>(request, warmup_callback.clone())
                .unwrap();
        }
    }

    // drain warm-up
    for _ in 0..concurrent_rqs {
        semaphore.acquire();
    }

    // ── Measurement phase ──
    info!(
        "Executing experiment ({} ops)...",
        benchmark_config.ops_number() / 2
    );

    let start = Instant::now();
    let seq = Arc::new(AtomicU64::new(0));
    let meas_sem = semaphore.clone();

    for _ in 0..(benchmark_config.ops_number() / 2) {
        semaphore.acquire();

        let op = generator.next_op(force_ordered);

        // Which per-kind tracker this request also reports to. The aggregate
        // CRUD_CLIENT_LATENCY is dominated by whichever kind the mix favours (reads, at the
        // default 70/15/10/5), and speculation does materially different work per kind --
        // a read is answered from the accumulated cache, a write accumulates a delta -- so
        // the aggregate alone cannot show where the win came from.
        let kind_metric_id = match &op.request_type {
            CRUDRequestType::Read { .. } => CRUD_LATENCY_READ_ID,
            CRUDRequestType::Create { .. } | CRUDRequestType::Update { .. } => {
                CRUD_LATENCY_WRITE_ID
            }
            CRUDRequestType::Delete { .. } => CRUD_LATENCY_DELETE_ID,
        };

        let correlation_id = format!("{}-{}", id, seq.fetch_add(1, Ordering::Relaxed));

        metric_correlation_time_start(CRUD_CLIENT_LATENCY_ID, &correlation_id);
        metric_correlation_time_start(kind_metric_id, &correlation_id);

        let sem_clone = meas_sem.clone();
        let corr_clone = correlation_id.clone();

        let callback = Arc::new(move |_reply| {
            metric_correlation_time_end(CRUD_CLIENT_LATENCY_ID, &corr_clone);
            metric_correlation_time_end(kind_metric_id, &corr_clone);
            metric_increment(CRUD_CLIENT_OPS_DONE_ID, Some(1));
            sem_clone.release();
        });

        let request = CRUDRequest::new(op.time_delay, op.request_type);

        if op.is_ordered {
            concurrent_client
                .update_imm_callback::<Ordered>(request, callback)
                .unwrap();
        } else {
            concurrent_client
                .update_imm_callback::<Unordered>(request, callback)
                .unwrap();
        }
    }

    // drain measurement
    for _ in 0..concurrent_rqs {
        semaphore.acquire();
    }

    let time_passed = start.elapsed();
    let ops_done = benchmark_config.ops_number() / 2;

    warn!(
        "{:?} // Done in {:?}. ({} ops/s)",
        concurrent_client.id(),
        time_passed,
        (ops_done * 1_000_000) / time_passed.as_micros() as usize
    );
}
