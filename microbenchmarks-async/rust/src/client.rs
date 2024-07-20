use crate::common::{ClientNode, ReconfProtocol, SMRClient, BFT, generate_log};
use crate::config::benchmark_configs::{BenchmarkConfig, read_benchmark_config};
use crate::serialize::{MicrobenchmarkData, Request, REQUEST, VERBOSE};
use atlas_client::client;
use atlas_client::client::ordered_client::Ordered;
use atlas_client::client::unordered_client::UnorderedClientMode;
use atlas_client::client::ClientConfig;
use atlas_client::concurrent_client::ConcurrentClient;
use atlas_common::async_runtime;
use atlas_default_configs::{get_network_configurations, get_reconfig_config};
use atlas_metrics::{with_metric_level, with_metrics, InfluxDBArgs, MetricLevel};
use chrono::Utc;
use semaphores::RawSemaphore;
use std::sync::Arc;
use std::time::{Duration, Instant};
use config::File;
use config::FileFormat::Toml;
use tracing::{debug, info, trace};

pub(super) fn setup_metrics(influx_db_args: InfluxDBArgs) {
    atlas_metrics::initialize_metrics(
        vec![
            with_metrics(atlas_communication::metric::metrics()),
            with_metrics(atlas_core::metric::metrics()),
            with_metrics(atlas_comm_mio::metrics::metrics()),
            with_metrics(atlas_client::metric::metrics()),
            with_metric_level(MetricLevel::Debug),
        ],
        influx_db_args,
    );
}

pub(super) fn client_main() {
    let benchmark = read_benchmark_config().expect("Failed to load benchmark config");

    setup_and_run_client(benchmark);
}

fn setup_and_run_client(benchmark_config: BenchmarkConfig) {
    let reconfig_config = get_reconfig_config().unwrap();
    let node_id = reconfig_config.node_id;

    let influx = atlas_default_configs::influx_db_settings::read_influx_db_config(File::new("config/influx_db.toml", Toml), Some(node_id)).unwrap();

    setup_metrics(influx.into());
    
    let _log_guard = generate_log(node_id.0);

    let (network_conf, pool_config) = get_network_configurations(node_id).unwrap();

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
    >(node_id, client_cfg)).unwrap();
    
    info!("Client initialized!");

    run_client(client, benchmark_config)
}

fn run_client(client: SMRClient, benchmark_config: BenchmarkConfig) {
    let concurrent_rqs = benchmark_config.concurrent_rqs();

    let id = u32::from(client.id());

    info!("Warm up...");

    let concurrent_client = ConcurrentClient::from_client(client, concurrent_rqs).unwrap();

    let semaphore = Arc::new(RawSemaphore::new(concurrent_rqs));

    let iterator = 0..(benchmark_config.ops_number() / 2);

    let mut ramp_up = 1000;

    let rq_sleep = Duration::from_millis(benchmark_config.request_sleep_millis() as u64);

    for req in iterator {
        //Only allow concurrent_rqs per client at the network
        semaphore.acquire();

        if *VERBOSE {
            trace!("{:?} // Sending req {}...", concurrent_client.id(), req);
        }

        let sem_clone = semaphore.clone();

        concurrent_client
            .update_callback::<Ordered>(
                Request::new(Arc::clone(&*REQUEST)),
                Box::new(move |reply| {
                    //Release another request for this client
                    sem_clone.release();

                    if *VERBOSE {
                        if req % 1000 == 0 {
                            trace!("{} // {} operations sent!", id, req);
                        }

                        trace!(" sent!");
                    }
                }),
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

        let last_send_instant = Utc::now();

        let sem_clone = semaphore.clone();

        concurrent_client
            .update_callback::<Ordered>(
                Request::new(Arc::clone(&*REQUEST)),
                Box::new(move |reply| {
                    //Release another request for this client
                    sem_clone.release();

                    if *VERBOSE {
                        if req % 1000 == 0 {
                            trace!("{} // {} operations sent!", id, req);
                        }

                        trace!(" sent!");
                    }
                }),
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

    info!(
        "{:?} // Test done in {:?}. ({} ops/s)",
        concurrent_client.id(),
        time_passed,
        (ops_done * 1_000_000) / time_passed.as_micros() as usize
    );
}
