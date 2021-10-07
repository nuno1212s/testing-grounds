use crate::common::*;
use crate::data::Update;
use crate::serialize::YcsbData;

use std::sync::Arc;
use std::time::Duration;
use std::sync::atomic::{Ordering, AtomicI32};

use futures_timer::Delay;

use febft::bft::threadpool;
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
        rt::block_on(async_main());
    } else {
        rt::block_on(client_async_main());
    }
}

async fn async_main() {
    let clients_config = parse_config("../config/clients.config").unwrap();
    let replicas_config = parse_config("../config/replicas.config").unwrap();

    let mut secret_keys: HashMap<NodeId, KeyPair> = sk_stream()
        .take(replicas_config.len())
        .enumerate()
        .map(|(id, sk)| (NodeId::from(id), sk))
        .collect();
    let public_keys: HashMap<NodeId, PublicKey> = secret_keys
        .iter()
        .map(|(id, sk)| (*id, sk.public_key().into()))
        .collect();

    for replica in &replicas_config {
        let id = NodeId::from(replica.id);
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
        rt::spawn(async move {
            println!("Bootstrapping replica #{}", u32::from(id));
            let mut replica = fut.await.unwrap();
            println!("Running replica #{}", u32::from(id));
            replica.run().await.unwrap();
        });
    }
    drop((secret_keys, public_keys, clients_config, replicas_config));

    // run forever
    std::future::pending().await
}

async fn client_async_main() {
    let clients_config = parse_config("../config/clients.config").unwrap();
    let replicas_config = parse_config("../config/replicas.config").unwrap();

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

    let throughput = Arc::new(AtomicI32::new(0));
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

    let mut clients = Vec::with_capacity(clients_config.len());
    for _i in 0..clients_config.len() {
        clients.push(rx.recv().await.unwrap());
    }
    for client in clients {
        let throughput = Arc::clone(&throughput);
        rt::spawn(run_client(client, throughput));
    }
    drop(clients_config);

    // run for 30 seconds
    println!("Running tests...");
    Delay::new(Duration::from_secs(30)).await;
    let result = throughput.load(Ordering::Relaxed);
    println!("Done running");

    println!("Throughput: {} ops", result);
    println!("Throughput per sec: {} ops", result / 30);
}

fn sk_stream() -> impl Iterator<Item = KeyPair> {
    std::iter::repeat_with(|| {
        // only valid for ed25519!
        let buf = [0; 32];
        KeyPair::from_bytes(&buf[..]).unwrap()
    })
}

async fn run_client(mut client: Client<YcsbData>, throughput: Arc<AtomicI32>) {
    const LEN: usize = 1024;
    let mut s = String::new();
    let mut ch = 0u8;
    loop {
        s.push(ch as char);
        ch += 1;
        let mut values = collections::hash_map();
        let key = String::from_utf8(vec![0; LEN]).unwrap();
        let value = vec![0; LEN];
        values.insert(key, value);
        let request = Update {
            table: s.clone(),
            key: s.clone(),
            values,
        };
        if ch % 128 == 0 {
            ch = 0;
        } else {
            s.pop();
        }
        client.update(request).await;
        throughput.fetch_add(1, Ordering::Relaxed);
    }
}
