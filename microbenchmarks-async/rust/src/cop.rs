use std::collections::LinkedList;
use crate::common::*;
use crate::serialize::MicrobenchmarkData;

use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use intmap::IntMap;
use chrono::offset::Utc;
use futures_timer::Delay;
use rand_core::{OsRng, RngCore};
use nolock::queues::mpsc::jiffy::{
    async_queue,
    AsyncSender,
};

use febft::bft::communication::{channel, PeerAddr};
use febft::bft::core::client::Client;
use febft::bft::communication::NodeId;
use febft::bft::async_runtime as rt;
use febft::bft::{
    init,
    InitConfig,
};
use febft::bft::crypto::signature::{
    KeyPair,
    PublicKey,
};
use febft::bft::benchmarks::{
    BenchmarkHelper,
    BenchmarkHelperStore,
};

pub fn main() {
    let is_client = std::env::var("CLIENT")
        .map(|x| x == "1")
        .unwrap_or(false);

    let conf = InitConfig {
        //Divide the logical cores into the thread pool and the async threadpool.
        //This should leave enough room for the threads that each replica requires to constantly
        //Have (which we want to avoid context switching on)
        pool_threads: num_cpus::get() / 4,
        async_threads: if is_client { num_cpus::get() } else { num_cpus::get() / 2 },
    };

    let _guard = unsafe { init(conf).unwrap() };
    if !is_client {
        let replica_id: u32 = std::env::var("ID")
            .iter()
            .flat_map(|id| id.parse())
            .next()
            .unwrap();

        main_(NodeId::from(replica_id));
    } else {
        rt::block_on(client_async_main());
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

        let fut = setup_replica(
            replicas_config.len(),
            id,
            sk,
            addrs,
            public_keys.clone(),
        );

        println!("Bootstrapping replica #{}", u32::from(id));
        let replica = rt::block_on(fut).unwrap();
        println!("Running replica #{}", u32::from(id));

        //Here we want to launch a statistics thread for each replica since they are on different machines
        crate::os_statistics::start_statistics_thread(id);

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

    let (tx, mut rx) = channel::new_bounded(8);

    let mut first_cli = u32::MAX;

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
    crate::os_statistics::start_statistics_thread(NodeId(first_cli));

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

async fn run_client(client: Client<MicrobenchmarkData>, q: Arc<AsyncSender<String>>) {
    let concurrent_rqs: usize = MicrobenchmarkData::CONCURRENT_RQS;

    let mut sessions = Vec::with_capacity(concurrent_rqs);

    let id = u32::from(client.id());

    println!("Warm up...");

    for _session in 0..concurrent_rqs {
        let mut ramp_up: i32 = 1000;

        let mut client = client.clone();

        let request = Arc::new({
            let mut r = vec![0; MicrobenchmarkData::REQUEST_SIZE];
            OsRng.fill_bytes(&mut r);
            r
        });

        let q = q.clone();

        let join_handle = rt::spawn(async move {
            let iterator = 0..(MicrobenchmarkData::OPS_NUMBER / 2 / concurrent_rqs);

            for req in iterator {
                if MicrobenchmarkData::VERBOSE {
                    print!("Sending req {:?}/{}...", client.session(), req);
                }

                let last_send_instant = Utc::now();

                client.update(Arc::downgrade(&request)).await;

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
                    if req % 1000 == 0 {
                        println!("{} // {} operations sent!", id, req);
                    }

                    println!(" sent!");
                }

                if MicrobenchmarkData::REQUEST_SLEEP_MILLIS != Duration::ZERO {
                    Delay::new(MicrobenchmarkData::REQUEST_SLEEP_MILLIS).await;
                } else if ramp_up > 0 {
                    Delay::new(Duration::from_millis(ramp_up as u64)).await;
                }

                ramp_up -= 100;
            }
        });

        sessions.push(join_handle);
    }

    for mut x in sessions {
        x.await;
    }

    let mut sessions = Vec::with_capacity(concurrent_rqs);

    println!("Executing experiment for {} ops", MicrobenchmarkData::OPS_NUMBER / 2);

    for _session in 0..concurrent_rqs {
        let mut client = client.clone();

        let request = Arc::new({
            let mut r = vec![0; MicrobenchmarkData::REQUEST_SIZE];
            OsRng.fill_bytes(&mut r);
            r
        });

        let q = q.clone();

        let mut st = BenchmarkHelper::new(client.id(), MicrobenchmarkData::OPS_NUMBER / 2);

        let join_handle = rt::spawn(async move {
            let iterator = 0..(MicrobenchmarkData::OPS_NUMBER / 2 / concurrent_rqs);

            for req in iterator {
                if MicrobenchmarkData::VERBOSE {
                    print!("Sending req {:?}/{}...", client.session(), req);
                }

                let last_send_instant = Utc::now();

                client.update(Arc::downgrade(&request)).await;

                let exec_time = Utc::now();

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

                (exec_time, last_send_instant).store(&mut st);

                if MicrobenchmarkData::VERBOSE {
                    if req % 1000 == 0 {
                        println!("{} // {} operations sent!", id, req);
                    }

                    println!(" sent!");
                }

                if MicrobenchmarkData::REQUEST_SLEEP_MILLIS != Duration::ZERO {
                    Delay::new(MicrobenchmarkData::REQUEST_SLEEP_MILLIS).await;
                }
            }

            st
        });

        sessions.push(join_handle);
    }

    let mut st = sessions.pop().unwrap().await.unwrap();

    for x in sessions {
        x.await;
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