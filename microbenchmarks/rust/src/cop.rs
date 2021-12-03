use crate::common::*;
use crate::serialize::MicrobenchmarkData;

use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use chrono::offset::Utc;
use futures_timer::Delay;
use rand_core::{OsRng, RngCore};
use nolock::queues::mpsc::jiffy::{
    async_queue,
    AsyncSender,
};

use febft::bft::communication::channel;
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
use febft::bft::collections::{
    self,
    HashMap,
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
        pool_threads: num_cpus::get(),
        async_threads: num_cpus::get(),
    };
    let _guard = unsafe { init(conf).unwrap() };
    if !is_client {
        let replica_id: u32 = std::env::var("ID")
            .iter()
            .flat_map(|id| id.parse())
            .next()
            .unwrap();
        rt::block_on(async_main(NodeId::from(replica_id)));
    } else {
        rt::block_on(client_async_main());
    }
}

async fn async_main(id: NodeId) {
    let mut replica = {
        let clients_config = parse_config("./config/clients.config").unwrap();
        let replicas_config = parse_config("./config/replicas.config").unwrap();

        let mut secret_keys: HashMap<NodeId, KeyPair> = sk_stream()
            .take(clients_config.len())
            .enumerate()
            .map(|(id, sk)| (NodeId::from(1000 + id), sk))
            .chain(sk_stream()
                .take(replicas_config.len())
                .enumerate()
                .map(|(id, sk)| (NodeId::from(id), sk)))
            .collect();
        let public_keys: HashMap<NodeId, PublicKey> = secret_keys
            .iter()
            .map(|(id, sk)| (*id, sk.public_key().into()))
            .collect();

        let addrs = {
            let mut addrs = collections::hash_map();
            for other in &replicas_config {
                let id = NodeId::from(other.id);
                let addr = format!("{}:{}", other.ipaddr, other.portno);
                addrs.insert(id, crate::addr!(&other.hostname => addr));
            }
            for client in &clients_config {
                let id = NodeId::from(client.id);
                let addr = format!("{}:{}", client.ipaddr, client.portno);
                addrs.insert(id, crate::addr!(&client.hostname => addr));
            }
            addrs
        };
        let sk = secret_keys.remove(&id).unwrap();
        let fut = setup_replica(
            replicas_config.len(),
            id,
            sk,
            addrs,
            public_keys.clone(),
        );

        println!("Bootstrapping replica #{}", u32::from(id));
        let replica = fut.await.unwrap();
        println!("Running replica #{}", u32::from(id));
        replica
    };

    // run forever
    replica.run().await.unwrap();
}

async fn client_async_main() {
    let clients_config = parse_config("./config/clients.config").unwrap();
    let replicas_config = parse_config("./config/replicas.config").unwrap();

    let mut secret_keys: HashMap<NodeId, KeyPair> = sk_stream()
        .take(clients_config.len())
        .enumerate()
        .map(|(id, sk)| (NodeId::from(1000 + id), sk))
        .chain(sk_stream()
            .take(replicas_config.len())
            .enumerate()
            .map(|(id, sk)| (NodeId::from(id), sk)))
        .collect();
    let public_keys: HashMap<NodeId, PublicKey> = secret_keys
        .iter()
        .map(|(id, sk)| (*id, sk.public_key().into()))
        .collect();

    let (tx, mut rx) = channel::new_bounded(8);

    for client in &clients_config {
        let id = NodeId::from(client.id);
        let addrs = {
            let mut addrs = collections::hash_map();
            for replica in &replicas_config {
                let id = NodeId::from(replica.id);
                let addr = format!("{}:{}", replica.ipaddr, replica.portno);
                addrs.insert(id, crate::addr!(&replica.hostname => addr));
            }
            for other in &clients_config {
                let id = NodeId::from(other.id);
                let addr = format!("{}:{}", other.ipaddr, other.portno);
                addrs.insert(id, crate::addr!(&other.hostname => addr));
            }
            addrs
        };
        let sk = secret_keys.remove(&id).unwrap();
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

    let (mut queue, tx) = async_queue();
    let tx = Arc::new(tx);

    let mut clients = Vec::with_capacity(clients_config.len());
    for _i in 0..clients_config.len() {
        clients.push(rx.recv().await.unwrap());
    }
    let mut handles = Vec::with_capacity(clients_config.len());
    for client in clients {
        let tx = Arc::clone(&tx);
        let h = rt::spawn(run_client(client, tx));
        handles.push(h);
    }
    drop(clients_config);

    for (i, h) in handles.into_iter().enumerate() {
        eprintln!("Waiting on client {}", 1000 + i);
        let _ = h.await;
        eprintln!("Client {} returned", 1000 + i);
    }

    let mut file = File::create("./latencies.out").unwrap();

    eprintln!("Writing latencies");
    while let Ok(line) = queue.try_dequeue() {
        file.write_all(line.as_ref()).unwrap();
    }
    eprintln!("Done writing latencies");

    eprintln!("Flushing latencies");
    file.flush().unwrap();
    eprintln!("Done flushing latencies");
}

fn sk_stream() -> impl Iterator<Item = KeyPair> {
    std::iter::repeat_with(|| {
        // only valid for ed25519!
        let buf = [0; 32];
        KeyPair::from_bytes(&buf[..]).unwrap()
    })
}

async fn run_client(mut client: Client<MicrobenchmarkData>, q: Arc<AsyncSender<String>>) {
    let mut ramp_up: i32 = 1000;

    let request = Arc::new({
        let mut r = vec![0; MicrobenchmarkData::REQUEST_SIZE];
        OsRng.fill_bytes(&mut r);
        r
    });

    let iterator = 0..MicrobenchmarkData::OPS_NUMBER/2;

    println!("Warm up...");

    let id = u32::from(client.id());

    for req in iterator {
        if MicrobenchmarkData::VERBOSE {
            print!("Sending req {}...", req);
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

    let mut st = BenchmarkHelper::new(MicrobenchmarkData::OPS_NUMBER/2);

    println!("Executing experiment for {} ops", MicrobenchmarkData::OPS_NUMBER/2);

    let iterator = (MicrobenchmarkData::OPS_NUMBER/2)..MicrobenchmarkData::OPS_NUMBER;

    for req in iterator {
        if id == 1000 {
            eprintln!("Sending req {}", req);
        }

        if MicrobenchmarkData::VERBOSE {
            print!("{} // Sending req {}...", id, req);
        }

        let last_send_instant = Utc::now();
        client.update(Arc::downgrade(&request)).await;
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
            st.average(true) / 1000.0);

        println!("{} // Standard deviation for {} executions (-10%) = {} us",
            id,
            MicrobenchmarkData::OPS_NUMBER / 2,
            st.standard_deviation(true) / 1000.0);

        println!("{} // Average time for {} executions (all samples) = {} us",
            id,
            MicrobenchmarkData::OPS_NUMBER / 2,
            st.average(false) / 1000.0);

        println!("{} // Standard deviation for {} executions (all samples) = {} us",
            id,
            MicrobenchmarkData::OPS_NUMBER / 2,
            st.standard_deviation(false) / 1000.0);

        println!("{} // Maximum time for {} executions (all samples) = {} us",
            id,
            MicrobenchmarkData::OPS_NUMBER / 2,
            st.max(false) / 1000);
    }
}
