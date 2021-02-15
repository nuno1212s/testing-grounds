use std::sync::Arc;
use std::sync::atomic::{Ordering, AtomicBool};
use flume::{bounded, Receiver};
use tokio::runtime::Runtime;
use futures::select;

const CAP: usize = 32;
const SENDERS: usize = 100;

struct Rand {
    seed: u64,
}

impl Rand {
    fn new(seed: u64) -> Rand {
        Rand { seed }
    }
}

impl Iterator for Rand {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        self.seed = 6364136223846793005*self.seed + 1;
        Some((self.seed >> 33) as u32)
    }
}

////////////////////////////////////////////////////////////////////////////

#[bench]
fn bench_channel_1_1(b: &mut test::Bencher) {
    let rt = Runtime::new().unwrap();
    let _guard = rt.enter();
    let (quit, rx) = rt.block_on(bench_channel_1_1_setup());
    b.iter(move || rt.block_on(bench_channel_1_1_main(&rx)));
    quit.store(true, Ordering::Relaxed);
}

async fn bench_channel_1_1_setup() -> (Arc<AtomicBool>, Receiver<()>) {
    let (tx, rx) = bounded(CAP);
    let quit = Arc::new(AtomicBool::new(false));
    let quit_clone = Arc::clone(&quit);

    tokio::spawn(async move {
        let quit = quit_clone;
        for _i in 0..SENDERS {
            let tx = tx.clone();
            let quit = Arc::clone(&quit);
            tokio::spawn(async move {
                while !quit.load(Ordering::Relaxed) {
                    tx.send_async(()).await.unwrap();
                }
            });
        }
    });

    (quit, rx)
}

async fn bench_channel_1_1_main(rx: &Receiver<()>) {
    rx.recv_async().await.unwrap();
}

////////////////////////////////////////////////////////////////////////////

#[bench]
fn bench_channel_4_1(b: &mut test::Bencher) {
    let rt = Runtime::new().unwrap();
    let _guard = rt.enter();
    let (quit, rx) = rt.block_on(bench_channel_4_1_setup());
    b.iter(move || rt.block_on(bench_channel_4_1_main(&rx)));
    quit.store(true, Ordering::Relaxed);
}

async fn bench_channel_4_1_setup() -> (Arc<AtomicBool>, Vec<Receiver<()>>) {
    let (tx, rx) = (0..4)
        .map(|_| bounded(CAP))
        .fold((Vec::new(), Vec::new()), |(mut tx_acc, mut rx_acc), (tx, rx)| {
            tx_acc.push(tx);
            rx_acc.push(rx);
            (tx_acc, rx_acc)
        });
    let quit = Arc::new(AtomicBool::new(false));
    let quit_clone = Arc::clone(&quit);

    tokio::spawn(async move {
        let quit = quit_clone;
        for i in 0..SENDERS {
            let tx = tx.clone();
            let quit = Arc::clone(&quit);
            tokio::spawn(async move {
                let rand_indices = Rand::new((123456_u64).wrapping_mul((i+1) as u64))
                    .map(|x| (x % 4) as usize);
                for i in rand_indices {
                    if quit.load(Ordering::Relaxed) {
                        return;
                    }
                    tx[i].send_async(()).await.unwrap();
                }
            });
        }
    });

    (quit, rx)
}

async fn bench_channel_4_1_main(rx: &Vec<Receiver<()>>) {
    select! {
        _ = rx[0].recv_async() => (),
        _ = rx[1].recv_async() => (),
        _ = rx[2].recv_async() => (),
        _ = rx[3].recv_async() => (),
    }
}

////////////////////////////////////////////////////////////////////////////

#[bench]
fn bench_channel_8_1(b: &mut test::Bencher) {
    let rt = Runtime::new().unwrap();
    let _guard = rt.enter();
    let (quit, rx) = rt.block_on(bench_channel_8_1_setup());
    b.iter(move || rt.block_on(bench_channel_8_1_main(&rx)));
    quit.store(true, Ordering::Relaxed);
}

async fn bench_channel_8_1_setup() -> (Arc<AtomicBool>, Vec<Receiver<()>>) {
    let (tx, rx) = (0..8)
        .map(|_| bounded(CAP))
        .fold((Vec::new(), Vec::new()), |(mut tx_acc, mut rx_acc), (tx, rx)| {
            tx_acc.push(tx);
            rx_acc.push(rx);
            (tx_acc, rx_acc)
        });
    let quit = Arc::new(AtomicBool::new(false));
    let quit_clone = Arc::clone(&quit);

    tokio::spawn(async move {
        let quit = quit_clone;
        for i in 0..SENDERS {
            let tx = tx.clone();
            let quit = Arc::clone(&quit);
            tokio::spawn(async move {
                let rand_indices = Rand::new((123456_u64).wrapping_mul((i+1) as u64))
                    .map(|x| (x % 8) as usize);
                for i in rand_indices {
                    if quit.load(Ordering::Relaxed) {
                        return;
                    }
                    tx[i].send_async(()).await.unwrap();
                }
            });
        }
    });

    (quit, rx)
}

async fn bench_channel_8_1_main(rx: &Vec<Receiver<()>>) {
    select! {
        _ = rx[0].recv_async() => (),
        _ = rx[1].recv_async() => (),
        _ = rx[2].recv_async() => (),
        _ = rx[3].recv_async() => (),
        _ = rx[4].recv_async() => (),
        _ = rx[5].recv_async() => (),
        _ = rx[6].recv_async() => (),
        _ = rx[7].recv_async() => (),
    }
}
