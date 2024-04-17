use std::sync::Arc;
use std::time::Instant;

use tracing::info;
use rand::distributions::Distribution;
use rand_core::SeedableRng;
use rand_xoshiro::SplitMix64;
use semaphores::RawSemaphore;

use atlas_client::client::ordered_client::Ordered;
use atlas_client::client::unordered_client::Unordered;
use atlas_client::concurrent_client::ConcurrentClient;
use crate::CANCELED;

use crate::common::{get_concurrent_rqs, SMRClient};
use crate::serialize::{BERequest, MicrobenchmarkData};
use crate::workload_gen::{Generator, OpStandard, Operation};

pub(super) fn run_client(client: SMRClient, generator: Arc<Generator>, value: Arc<str>) {
    let id = client.id().0.clone();
    let concurrent_requests = get_concurrent_rqs();
    let op_count = MicrobenchmarkData::get_ops_number();
    let concurrent_client = ConcurrentClient::from_client(client, concurrent_requests).unwrap();
    
    let start = Instant::now();
    
    let sem = Arc::new(RawSemaphore::new(concurrent_requests));

    let mut rand = SplitMix64::seed_from_u64((6453 + (id * 1242)).into());

    info!("Starting client {:?}", concurrent_client.id());

    let op_sampler = OpStandard::default();

    for _ in 0..op_count {
        
        if CANCELED.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }

        let key = generator.get_key_zipf(&mut rand);
        let value = value.clone();
        let operation = op_sampler.sample(&mut rand);

        sem.acquire();
        
        let request = match operation {
            Operation::Read => BERequest::Read(key),
            Operation::Insert => BERequest::Write(key, value),
            Operation::Remove => BERequest::Delete(key),
            Operation::Update => BERequest::Write(key, value),
        };
        
        let sem_clone = sem.clone();

        match &request {
            BERequest::Read(_) => {
                concurrent_client
                    .update_callback::<Unordered>(
                        request,
                        Box::new(move |reply| {
                            sem_clone.release();
                        }),
                    )
                    .expect("Failed to send unordered request");
            }
            _ => {
                concurrent_client
                    .update_callback::<Ordered>(
                        request,
                        Box::new(move |reply| {
                            sem_clone.release();
                        }),
                    )
                    .expect("Failed to send ordered request");
            }
        }
    }
    
    for _ in 0..concurrent_requests {
        sem.acquire();
    }

    let time_passed = start.elapsed();

    let ops_done = MicrobenchmarkData::get_ops_number() / 2;

    println!("{:?} // Done.", concurrent_client.id());

    println!(
        "{:?} // Test done in {:?}. ({} ops/s)",
        concurrent_client.id(),
        time_passed,
        (ops_done * 1_000_000) / time_passed.as_micros() as usize
    );
}
