use std::env;
use crate::common::*;
use crate::serialize::{MicrobenchmarkData, Request};

use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration};

use intmap::IntMap;
use chrono::offset::Utc;
use konst::primitive::{parse_usize};
use nolock::queues::mpsc::jiffy::{
    async_queue,
    AsyncSender,
};

use semaphores::RawSemaphore;
use febft_client::client::Client;
use febft_client::client::ordered_client::Ordered;
use febft_common::crypto::signature::{KeyPair, PublicKey};
use febft_common::{async_runtime as rt, channel, init, InitConfig};
use febft_common::node_id::NodeId;
use febft_communication::tcpip::{PeerAddr};
use febft_metrics::with_metrics;

pub fn main() {
    let is_client = std::env::var("CLIENT")
        .map(|x| x == "1")
        .unwrap_or(false);

    let threadpool_threads = parse_usize(std::env::var("THREADPOOL_THREADS")
        .unwrap_or(String::from("2")).as_str()).unwrap();
    let async_threads = parse_usize(std::env::var("ASYNC_THREADS")
        .unwrap_or(String::from("2")).as_str()).unwrap();

    if !is_client {
        let replica_id: u32 = std::env::var("ID")
            .iter()
            .flat_map(|id| id.parse())
            .next()
            .unwrap();

        let conf = InitConfig {
            //If we are the client, we want to have many threads to send stuff to replicas
            threadpool_threads,
            async_threads,
            //If we are the client, we don't want any threads to send to other clients as that will never happen
            id: Some(replica_id.to_string()),
        };

        let _guard = unsafe { init(conf).unwrap() };
        let node_id = NodeId::from(replica_id);

        febft_metrics::initialize_metrics(vec![with_metrics(febft_pbft_consensus::bft::metric::metrics()),
                                               with_metrics(febft_messages::metric::metrics())],
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

        febft_metrics::initialize_metrics(vec![with_metrics(febft_pbft_consensus::bft::metric::metrics()),
                                               with_metrics(febft_messages::metric::metrics()),
                                               with_metrics(febft_client::metric::metrics())],
                                          influx_db_config(NodeId::from(first_id)));

        client_async_main();
    }
}

fn main_(id: NodeId) {
    let mut replica = {
        println!("Started working on the replica");

        //TODO: don't have this hardcoded?
        let first_cli = NodeId::from(1000u32);

        let clients_config = parse_config("./config/clients.config").unwrap();
        let replicas_config = parse_config("./config/replicas.config").unwrap();

        println!("Finished reading replica config.");

        let mut secret_keys: IntMap<KeyPair> = sk_stream()
            .take(clients_config.len())
            .enumerate()
            .map(|(id, sk)| (u64::from(first_cli) + id as u64, sk))
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

        /* let comm_stats = Some(Arc::new(CommStats::new(id,
                                                 first_cli,
                                                 MicrobenchmarkData::MEASUREMENT_INTERVAL))); */

        let comm_stats = None;

        let sk = secret_keys.remove(id.into()).unwrap();

        let fut = setup_replica(
            replicas_config.len(),
            id,
            sk,
            addrs,
            public_keys.clone(),
            comm_stats,
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

fn client_async_main() {
    let clients_config = parse_config("./config/clients.config").unwrap();
    let replicas_config = parse_config("./config/replicas.config").unwrap();

    let mut first_id: u32 = env::var("ID").unwrap_or(String::from("1000")).parse().unwrap();

    let client_count: u32 = env::var("NUM_CLIENTS").unwrap_or(String::from("1")).parse().unwrap();

    let mut secret_keys: IntMap<KeyPair> = sk_stream()
        .take(clients_config.len())
        .enumerate()
        .map(|(id, sk)| (first_id as u64 + id as u64, sk))
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

    /*let comm_stats = Some(Arc::new(CommStats::new(NodeId::from(first_id),
                                             NodeId::from(1000u32),
                                             MicrobenchmarkData::MEASUREMENT_INTERVAL)));*/

    let comm_stats = None;

    for i in 0..client_count {
        let id = NodeId::from(first_id + i);

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
            comm_stats.clone(),
        );

        let mut tx = tx.clone();
        rt::spawn(async move {
            //We can have this async initializer no problem, just don't want it to be used to actually send
            //The requests/control the clients
            println!("Bootstrapping client #{}", u32::from(id));
            let client = fut.await.unwrap();
            println!("Done bootstrapping client #{}", u32::from(id));
            tx.send(client).await.unwrap();
        });
    }

    drop((secret_keys, public_keys, replicas_config));

    let (mut queue, queue_tx) = async_queue();
    let queue_tx = Arc::new(queue_tx);

    let mut clients = Vec::with_capacity(client_count as usize);

    for _i in 0..client_count {
        clients.push(rt::block_on(rx.recv()).unwrap());
    }

    let mut handles = Vec::with_capacity(client_count as usize);

    for client in clients {
        let queue_tx = Arc::clone(&queue_tx);
        let id = client.id();

        let h = std::thread::Builder::new()
            .name(format!("Client {:?}", client.id()))
            .spawn(move || { run_client(client, queue_tx) })
            .expect(format!("Failed to start thread for client {:?} ", &id.id()).as_str());

        handles.push(h);

        // Delay::new(Duration::from_millis(5)).await;
    }

    drop(clients_config);

    //Start the OS resource monitoring thread
    //crate::os_statistics::start_statistics_thread(NodeId(first_id));

    for h in handles {
        let _ = h.join();
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

fn run_client(mut client: Client<MicrobenchmarkData, ClientNetworking>, q: Arc<AsyncSender<String>>) {
    let concurrent_rqs: usize = get_concurrent_rqs();

    let id = u32::from(client.id());

    println!("Warm up...");

    let request = Arc::new({
        let mut r = vec![0; MicrobenchmarkData::REQUEST_SIZE];
        r
    });

    let semaphore = Arc::new(RawSemaphore::new(concurrent_rqs));

    let iterator = 0..(MicrobenchmarkData::OPS_NUMBER / 2);

    let mut ramp_up = 1000;

    for req in iterator {

        //Only allow concurrent_rqs per client at the network
        semaphore.acquire();

        if MicrobenchmarkData::VERBOSE {
            println!("{:?} // Sending req {}...", client.id(), req);
        }

        let last_send_instant = Utc::now();

        let sem_clone = semaphore.clone();

        let q = q.clone();

        client.clone().update_callback::<Ordered>(Request::new(MicrobenchmarkData::REQUEST), Box::new(move |reply| {

            //Release another request for this client
            sem_clone.release();

            let latency = Utc::now()
                .signed_duration_since(last_send_instant)
                .num_nanoseconds()
                .unwrap_or(i64::MAX);

            let time_ms = Utc::now().timestamp_millis();

            /*let _ = q.enqueue(
                format!(
                    "{}\t{}\t{}\n",
                    id,
                    time_ms,
                    latency,
                ),
            );*/

            if MicrobenchmarkData::VERBOSE {
                if req % 1000 == 0 {
                    println!("{} // {} operations sent!", id, req);
                }

                println!(" sent!");
            }
        }));

        if MicrobenchmarkData::REQUEST_SLEEP_MILLIS != Duration::ZERO {
            std::thread::sleep(MicrobenchmarkData::REQUEST_SLEEP_MILLIS);
        } else if ramp_up > 0 {
            let to_sleep = fastrand::u32(ramp_up / 2..ramp_up);
            std::thread::sleep(Duration::from_millis(to_sleep as u64));

            ramp_up -= 100;
        }
    }

    println!("Executing experiment for {} ops", MicrobenchmarkData::OPS_NUMBER / 2);

    let iterator = 0..(MicrobenchmarkData::OPS_NUMBER / 2);

    //let mut st = BenchmarkHelper::new(client.id(), MicrobenchmarkData::OPS_NUMBER / 2);

    for req in iterator {
        semaphore.acquire();

        if MicrobenchmarkData::VERBOSE {
            print!("Sending req {}...", req);
        }

        let q = q.clone();

        let last_send_instant = Utc::now();

        let sem_clone = semaphore.clone();

        client.clone().update_callback::<Ordered>(Request::new(MicrobenchmarkData::REQUEST),
                                                  Box::new(move |reply| {

                                                      //Release another request for this client
                                                      sem_clone.release();

                                                      /* let latency = Utc::now()
                                                          .signed_duration_since(last_send_instant)
                                                          .num_nanoseconds()
                                                          .unwrap_or(i64::MAX);

                                                      let time_ms = Utc::now().timestamp_millis();*/

                                                      /*let _ = q.enqueue(
                                                          format!(
                                                              "{}\t{}\t{}\n",
                                                              id,
                                                              time_ms,
                                                              latency,
                                                          ),
                                                      );*/

                                                      //(exec_time, last_send_instant).store(st);

                                                      if MicrobenchmarkData::VERBOSE {
                                                          if req % 1000 == 0 {
                                                              println!("{} // {} operations sent!", id, req);
                                                          }

                                                          println!(" sent!");
                                                      }
                                                  }));

        if MicrobenchmarkData::REQUEST_SLEEP_MILLIS != Duration::ZERO {
            std::thread::sleep(MicrobenchmarkData::REQUEST_SLEEP_MILLIS);
        }
    }

    //Wait for all requests to finish
    for _ in 0..concurrent_rqs {
        semaphore.acquire();
    }

    println!("{:?} // Done.", client.id());

    /*
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
    */
}
