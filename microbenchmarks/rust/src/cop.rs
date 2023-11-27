use std::env;
use crate::common::*;
use crate::serialize::{MicrobenchmarkData, Request};

use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;

use intmap::IntMap;
use chrono::offset::Utc;
use futures_timer::Delay;
use konst::primitive::parse_usize;
use log4rs::append::Append;
use log4rs::append::console::ConsoleAppender;
use log4rs::append::rolling_file::policy::compound::CompoundPolicy;
use log4rs::append::rolling_file::policy::compound::roll::fixed_window::FixedWindowRoller;
use log4rs::append::rolling_file::{LogFile, RollingFileAppender};
use log4rs::{Config, Logger};
use log4rs::append::rolling_file::policy::compound::trigger::Trigger;
use log4rs::config::{Appender, Root};
use log4rs::encode::pattern::PatternEncoder;
use log4rs::filter::threshold::ThresholdFilter;
use log::LevelFilter;
use rand_core::{OsRng, RngCore};
use nolock::queues::mpsc::jiffy::{
    async_queue,
    AsyncSender,
};
use atlas_client::client::Client;
use atlas_client::client::ordered_client::Ordered;
use atlas_common::{channel, init, InitConfig};
use atlas_common::node_id::NodeId;
use atlas_common::async_runtime as rt;
use atlas_common::crypto::signature::{KeyPair, PublicKey};
use atlas_common::peer_addr::PeerAddr;
use atlas_metrics::benchmarks::{BenchmarkHelper, BenchmarkHelperStore, CommStats};
use atlas_metrics::{MetricLevel, with_metric_level, with_metrics};


#[derive(Debug)]
pub struct InitTrigger {
    has_been_triggered: AtomicBool,
}

impl Trigger for InitTrigger {
    fn trigger(&self, file: &LogFile) -> anyhow::Result<bool> {
        if file.len_estimate() == 0 {
            println!("Not triggering rollover because file is empty");

            self.has_been_triggered.store(true, Relaxed);
            return Ok(false);
        }

        Ok(self.has_been_triggered.compare_exchange(false, true, Relaxed, Relaxed).is_ok())
    }
}

fn format_old_log(id: u32, str: &str) -> String {
    format!("./logs/log_{}/old/febft{}.log.{}", id, str, "{}")
}

fn format_log(id: u32, str: &str) -> String {
    format!("./logs/log_{}/febft{}.log", id, str)
}

fn policy(id: u32, str: &str) -> CompoundPolicy {
    let trigger = InitTrigger { has_been_triggered: AtomicBool::new(false) };

    let roller = FixedWindowRoller::builder().base(1).build(format_old_log(id, str).as_str(), 5).unwrap();

    CompoundPolicy::new(Box::new(trigger), Box::new(roller))
}

fn file_appender(id: u32, str: &str) -> Box<dyn Append> {
    Box::new(RollingFileAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{l} {d} - {m}{n}")))
        .build(format_log(id, str).as_str(), Box::new(policy(id, str))).unwrap())
}

fn generate_log(id: u32) {
    let console_appender = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{l} {d} - {m}{n}"))).build();

    let config = Config::builder()
        .appender(Appender::builder().build("comm", file_appender(id, "_comm")))
        .appender(Appender::builder().build("reconfig", file_appender(id, "_reconfig")))
        .appender(Appender::builder().build("common", file_appender(id, "_common")))
        .appender(Appender::builder().build("consensus", file_appender(id, "_consensus")))
        .appender(Appender::builder().build("file", file_appender(id, "")))
        .appender(Appender::builder().build("log_transfer", file_appender(id, "_log_transfer")))
        .appender(Appender::builder().build("state_transfer", file_appender(id, "_state_transfer")))
        .appender(Appender::builder().build("decision_log", file_appender(id, "_decision_log")))
        .appender(Appender::builder().build("replica", file_appender(id, "_replica")))
        .appender(Appender::builder().build("view_transfer", file_appender(id, "_view_transfer")))
        .appender(Appender::builder().filter(Box::new(ThresholdFilter::new(LevelFilter::Warn))).build("console", Box::new(console_appender)))

        .logger(Logger::builder().appender("comm").build("atlas_communication", LevelFilter::Debug))
        .logger(Logger::builder().appender("common").build("atlas_common", LevelFilter::Debug))
        .logger(Logger::builder().appender("reconfig").build("atlas_reconfiguration", LevelFilter::Debug))
        .logger(Logger::builder().appender("log_transfer").build("atlas_log_transfer", LevelFilter::Debug))
        .logger(Logger::builder().appender("decision_log").build("atlas_decision_log", LevelFilter::Debug))
        .logger(Logger::builder().appender("replica").build("atlas_smr_replica", LevelFilter::Debug))
        .logger(Logger::builder().appender("consensus").build("febft_pbft_consensus", LevelFilter::Debug))
        .logger(Logger::builder().appender("state_transfer").build("febft_state_transfer", LevelFilter::Debug))
        .logger(Logger::builder().appender("view_transfer").build("atlas_view_transfer", LevelFilter::Debug))
        .build(Root::builder().appender("file").build(LevelFilter::Debug), ).unwrap();

    let _handle = log4rs::init_config(config).unwrap();
}

pub fn main() {
    let is_client = std::env::var("CLIENT")
        .map(|x| x == "1")
        .unwrap_or(false);

    let threadpool_threads = parse_usize(std::env::var("THREADPOOL_THREADS")
        .unwrap_or(String::from("2")).as_str()).unwrap();
    let async_threads = parse_usize(std::env::var("ASYNC_THREADS")
        .unwrap_or(String::from("2")).as_str()).unwrap();

    let id: u32 = std::env::var("ID")
        .iter()
        .flat_map(|id| id.parse())
        .next()
        .unwrap();


    if !is_client {
        let id: u32 = std::env::var("ID")
            .iter()
            .flat_map(|id| id.parse())
            .next()
            .unwrap();

        generate_log(id);

        let conf = InitConfig {
            //If we are the client, we want to have many threads to send stuff to replicas
            threadpool_threads,
            async_threads,
            //If we are the client, we don't want any threads to send to other clients as that will never happen
            id: Some(id.to_string()),
        };

        let _guard = unsafe { init(conf).unwrap() };
        let node_id = NodeId::from(id);

        atlas_metrics::initialize_metrics(vec![with_metrics(febft_pbft_consensus::bft::metric::metrics()),
                                               with_metrics(atlas_core::metric::metrics()),
                                               with_metrics(atlas_communication::metric::metrics()),
                                               with_metrics(atlas_smr_replica::metric::metrics()),
                                               with_metrics(atlas_log_transfer::metrics::metrics()),
                                               with_metrics(febft_state_transfer::metrics::metrics()),
                                               with_metrics(atlas_view_transfer::metrics::metrics()),
                                               with_metric_level(MetricLevel::Trace)],
                                          influx_db_config(node_id));

        main_(node_id);
    } else {
        let conf = InitConfig {
            //If we are the client, we want to have many threads to send stuff to replicas
            threadpool_threads,
            async_threads,
            //If we are the client, we don't want any threads to send to other clients as that will never happen
            id: None,
        };

        let _guard = unsafe { init(conf).unwrap() };

        let mut first_id: u32 = env::var("ID").unwrap_or(String::from("1000")).parse().unwrap();

        atlas_metrics::initialize_metrics(vec![with_metrics(atlas_communication::metric::metrics()),
                                               with_metrics(atlas_client::metric::metrics()),
                                               with_metric_level(MetricLevel::Info)],
                                          influx_db_config(NodeId::from(first_id)));

        client_async_main();
    }
}

fn main_(id: NodeId) {
    let mut replica = {
        println!("Started working on the replica");

        let clients_config = parse_config("./config/clients.config").unwrap();
        let replicas_config = parse_config("./config/replicas.config").unwrap();

        println!("Finished reading replica config.");

        let mut secret_keys: IntMap<KeyPair> = sk_stream()
            .take(clients_config.len())
            .enumerate()
            .map(|(id, sk)| (1000 + id as u64, sk))
            .chain(sk_stream()
                .take(replicas_config.len())
                .enumerate()
                .map(|(id, sk)| (id as u64, sk)))
            .collect();

        let public_keys: IntMap<PublicKey> = secret_keys
            .iter()
            .map(|(id, sk)| (*id, sk.public_key().into()))
            .collect();

        println!("Finished reading keys.");

        let addrs = {
            let mut addrs = IntMap::new();
            for replica in &replicas_config {
                let id = NodeId::from(replica.id);
                let addr = format!("{}:{}", replica.ipaddr, replica.portno);
                let replica_addr = format!("{}:{}", replica.ipaddr, replica.rep_portno.unwrap());

                let replica_p_addr = PeerAddr::new_replica(crate::addr!(&replica.hostname => addr),
                                                           crate::addr!(&replica.hostname => replica_addr));

                addrs.insert(id.into(), replica_p_addr);
            }

            for other in &clients_config {
                let id = NodeId::from(other.id);
                let addr = format!("{}:{}", other.ipaddr, other.portno);

                let client_addr = PeerAddr::new(crate::addr!(&other.hostname => addr));

                addrs.insert(id.into(), client_addr);
            }

            addrs
        };

        let sk = secret_keys.remove(id.into()).unwrap();

        let first_cli = NodeId::from(1000u32);

        let comm_stats = Arc::new(CommStats::new(id,
                                                 first_cli,
                                                 MicrobenchmarkData::MEASUREMENT_INTERVAL));

        let fut = setup_replica(
            replicas_config.len(),
            id,
            sk,
            addrs,
            public_keys.clone(),
            Some(comm_stats),
        );

        println!("Bootstrapping replica #{}", u32::from(id));
        let replica = rt::block_on(fut).unwrap();
        println!("Running replica #{}", u32::from(id));

        //Here we want to launch a statistics thread for each replica since they are on different machines
        //crate::os_statistics::start_statistics_thread(id);

        replica
    };

    // run forever
    replica.run().unwrap();
}

async fn client_async_main() {
    let clients_config = parse_config("./config/clients.config").unwrap();
    let replicas_config = parse_config("./config/replicas.config").unwrap();

    let mut secret_keys: IntMap<KeyPair> = sk_stream()
        .take(clients_config.len())
        .enumerate()
        .map(|(id, sk)| (1000 + id as u64, sk))
        .chain(sk_stream()
            .take(replicas_config.len())
            .enumerate()
            .map(|(id, sk)| (id as u64, sk)))
        .collect();
    let public_keys: IntMap<PublicKey> = secret_keys
        .iter()
        .map(|(id, sk)| (*id, sk.public_key().into()))
        .collect();

    let (tx, mut rx) = channel::new_bounded_async(8);

    let mut first_cli = u32::MAX;

    for client in &clients_config {
        let id = NodeId::from(client.id);

        if client.id < first_cli {
            first_cli = client.id;
        }
    }

    let comm_stats = Arc::new(CommStats::new(
        NodeId::from(first_cli),
        NodeId::from(first_cli),
        MicrobenchmarkData::MEASUREMENT_INTERVAL
    ));

    for client in &clients_config {
        let id = NodeId::from(client.id);

        if client.id < first_cli {
            first_cli = client.id;
        }

        let addrs = {
            let mut addrs = IntMap::new();
            for replica in &replicas_config {
                let id = NodeId::from(replica.id);
                let addr = format!("{}:{}", replica.ipaddr, replica.portno);
                let replica_addr = format!("{}:{}", replica.ipaddr, replica.rep_portno.unwrap());

                let replica_p_addr = PeerAddr::new_replica(crate::addr!(&replica.hostname => addr),
                                                           crate::addr!(&replica.hostname => replica_addr));

                addrs.insert(id.into(), replica_p_addr);
            }

            for other in &clients_config {
                let id = NodeId::from(other.id);
                let addr = format!("{}:{}", other.ipaddr, other.portno);

                let client_addr = PeerAddr::new(crate::addr!(&other.hostname => addr));

                addrs.insert(id.into(), client_addr);
            }

            addrs
        };

        let sk = secret_keys.remove(id.into()).unwrap();

        let fut = setup_client(
            replicas_config.len(),
            id,
            sk,
            addrs,
            public_keys.clone(),
            Some(comm_stats.clone()),
        );

        let mut tx = tx.clone();
        rt::spawn(async move {
            println!("Bootstrapping client #{}", u32::from(id));
            let client = fut.await.unwrap();
            println!("Done bootstrapping client #{}", u32::from(id));
            tx.send(client).await.unwrap();
        });
    }
    drop((secret_keys, public_keys, replicas_config));

    let (mut queue, queue_tx) = async_queue();
    let queue_tx = Arc::new(queue_tx);

    let mut clients = Vec::with_capacity(clients_config.len());
    for _i in 0..clients_config.len() {
        clients.push(rx.recv().await.unwrap());
    }

    //Start the OS resource monitoring thread
    //crate::os_statistics::start_statistics_thread(NodeId(first_cli));

    let mut handles = Vec::with_capacity(clients_config.len());
    for client in clients {
        let queue_tx = Arc::clone(&queue_tx);
        let h = rt::spawn(run_client(client, queue_tx));
        handles.push(h);
    }
    drop(clients_config);

    for h in handles {
        let _ = h.await;
    }

    let mut file = File::create("./latencies.out").unwrap();

    while let Ok(line) = queue.try_dequeue() {
        file.write_all(line.as_ref()).unwrap();
    }

    file.flush().unwrap();
}

fn sk_stream() -> impl Iterator<Item=KeyPair> {
    std::iter::repeat_with(|| {
        // only valid for ed25519!
        let buf = [0; 32];
        KeyPair::from_bytes(&buf[..]).unwrap()
    })
}

async fn run_client(mut client: SMRClient, q: Arc<AsyncSender<String>>) {
    let mut ramp_up: i32 = 1000;

    let request = Arc::new({
        let mut r = vec![0; MicrobenchmarkData::REQUEST_SIZE];
        OsRng.fill_bytes(&mut r);
        r
    });

    let iterator = 0..MicrobenchmarkData::OPS_NUMBER / 2;

    println!("Warm up...");

    let id = u32::from(client.id());

    for req in iterator {
        if MicrobenchmarkData::VERBOSE {
            print!("Sending req {}...", req);
        }

        let last_send_instant = Utc::now();

        if let Err(error) = client.update::<Ordered>(Request::new(MicrobenchmarkData::REQUEST)).await {
            println!("Failed to send client update {:?}", error);
        }

        let latency = Utc::now()
            .signed_duration_since(last_send_instant)
            .num_nanoseconds()
            .unwrap_or(i64::MAX);

        let time_ms = Utc::now().timestamp_millis();

        let _ = q.enqueue(
            format!(
                "{}\t{}\t{}\n",
                id,
                time_ms,
                latency,
            ),
        );

        if MicrobenchmarkData::VERBOSE {
            println!(" sent!");
        }
        if MicrobenchmarkData::VERBOSE && (req % 1000 == 0) {
            println!("{} // {} operations sent!", id, req);
        }

        if MicrobenchmarkData::REQUEST_SLEEP_MILLIS != Duration::ZERO {
            Delay::new(MicrobenchmarkData::REQUEST_SLEEP_MILLIS).await;
        } else if ramp_up > 0 {
            Delay::new(Duration::from_millis(ramp_up as u64)).await;
        }
        ramp_up -= 100;
    }

    let mut st = BenchmarkHelper::new(client.id(), MicrobenchmarkData::OPS_NUMBER / 2);

    println!("Executing experiment for {} ops", MicrobenchmarkData::OPS_NUMBER / 2);

    let iterator = MicrobenchmarkData::OPS_NUMBER / 2..MicrobenchmarkData::OPS_NUMBER;

    for req in iterator {
        if MicrobenchmarkData::VERBOSE {
            print!("{} // Sending req {}...", id, req);
        }

        let last_send_instant = Utc::now();
        client.update::<Ordered>(Request::new(MicrobenchmarkData::REQUEST)).await;
        let exec_time = Utc::now();
        let latency = exec_time
            .signed_duration_since(last_send_instant)
            .num_nanoseconds()
            .unwrap_or(i64::MAX);

        let time_ms = Utc::now().timestamp_millis();

        let _ = q.enqueue(
            format!(
                "{}\t{}\t{}\n",
                id,
                time_ms,
                latency,
            ),
        );

        if MicrobenchmarkData::VERBOSE {
            println!(" sent!");
        }

        (exec_time, last_send_instant).store(&mut st);

        if MicrobenchmarkData::REQUEST_SLEEP_MILLIS != Duration::ZERO {
            Delay::new(MicrobenchmarkData::REQUEST_SLEEP_MILLIS).await;
        } else if ramp_up > 0 {
            Delay::new(Duration::from_millis(ramp_up as u64)).await;
        }
        ramp_up -= 100;

        if MicrobenchmarkData::VERBOSE && (req % 1000 == 0) {
            println!("{} // {} operations sent!", id, req);
        }
    }

    if id == 1000 {
        println!("{} // Average time for {} executions (-10%) = {} us",
                 id,
                 MicrobenchmarkData::OPS_NUMBER / 2,
                 st.average(true, false) / 1000.0);

        println!("{} // Standard deviation for {} executions (-10%) = {} us",
                 id,
                 MicrobenchmarkData::OPS_NUMBER / 2,
                 st.standard_deviation(true, true) / 1000.0);

        println!("{} // Average time for {} executions (all samples) = {} us",
                 id,
                 MicrobenchmarkData::OPS_NUMBER / 2,
                 st.average(false, true) / 1000.0);

        println!("{} // Standard deviation for {} executions (all samples) = {} us",
                 id,
                 MicrobenchmarkData::OPS_NUMBER / 2,
                 st.standard_deviation(false, true) / 1000.0);

        println!("{} // Maximum time for {} executions (all samples) = {} us",
                 id,
                 MicrobenchmarkData::OPS_NUMBER / 2,
                 st.max(false) / 1000);
    }
}
