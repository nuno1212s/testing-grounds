#![feature(test)]

extern crate test;

#[cfg(test)]
mod tests {
    use futures::select;
    use tokio::sync::watch;
    use tokio::runtime::Runtime;
    use flume::{bounded, Receiver};

    const CAP: usize = 32;
    const SENDERS: usize = 100;
    const BENCH_SIZE: usize = 1000;

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

    #[bench]
    fn bench_channel_4_1(b: &mut test::Bencher) {
        let rt = Runtime::new().unwrap();
        let _guard = rt.enter();
        let (quit, mut rx) = rt.block_on(bench_channel_4_1_setup());
        b.iter(move || rt.block_on(bench_channel_4_1_main(&mut rx)));
        quit.send(true).unwrap();
    }

    async fn bench_channel_4_1_setup() -> (watch::Sender<bool>, Vec<Receiver<()>>) {
        let (tx, rx) = (0..4)
            .map(|_| bounded(CAP))
            .fold((Vec::new(), Vec::new()), |(mut tx_acc, mut rx_acc), (tx, rx)| {
                tx_acc.push(tx);
                rx_acc.push(rx);
                (tx_acc, rx_acc)
            });
        let (quit, has_quit) = watch::channel(false);

        tokio::spawn(async move {
            for i in 0..SENDERS {
                let tx = tx.clone();
                let mut has_quit = has_quit.clone();
                tokio::spawn(async move {
                    let rand_indices = Rand::new((123456_u64).wrapping_mul((i+1) as u64))
                        .map(|x| (x % 4) as usize)
                        .take(BENCH_SIZE);
                    for i in rand_indices {
                        if let Ok(_) = has_quit.changed().await {
                            return;
                        }
                        tx[i].send_async(()).await.unwrap();
                    }
                });
            }
        });

        (quit, rx)
    }

    async fn bench_channel_4_1_main(rx: &mut Vec<Receiver<()>>) {
        select! {
            _ = rx[0].recv_async() => (),
            _ = rx[1].recv_async() => (),
            _ = rx[2].recv_async() => (),
            _ = rx[3].recv_async() => (),
        }
    }
}
