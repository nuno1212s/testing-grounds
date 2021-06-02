use crate::common::*;

use febft::bft::threadpool;
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
    let conf = InitConfig {
        async_threads: num_cpus::get(),
    };
    let _guard = unsafe { init(conf).unwrap() };
    rt::block_on(async_main());
}

async fn async_main() {
    let clients_config = parse_config("../config/clients.config").unwrap();
    let replicas_config = parse_config("../config/replicas.config").unwrap();

    let mut secret_keys: HashMap<NodeId, KeyPair> = sk_stream()
        .take(4)
        .enumerate()
        .map(|(id, sk)| (NodeId::from(id), sk))
        .collect();
    let public_keys: HashMap<NodeId, PublicKey> = secret_keys
        .iter()
        .map(|(id, sk)| (*id, sk.public_key().into()))
        .collect();

    let pool = threadpool::Builder::new()
        .num_threads(num_cpus::get() >> 1)
        .build();

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
            pool.clone(),
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
    drop((pool, secret_keys, public_keys));

    // run forever
    std::future::pending().await
}

fn sk_stream() -> impl Iterator<Item = KeyPair> {
    std::iter::repeat_with(|| {
        // only valid for ed25519!
        let buf = [0; 32];
        KeyPair::from_bytes(&buf[..]).unwrap()
    })
}
