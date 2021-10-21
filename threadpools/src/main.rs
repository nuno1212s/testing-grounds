use std::thread;
use std::sync::Arc;
use std::time::Duration;
use std::sync::atomic::{AtomicUsize, Ordering};

use cthpool::Builder as CThreadPoolBuilder;
use rayon::ThreadPoolBuilder as RayonBuilder;
use threadpool_crossbeam_channel::Builder as CrossbeamBuilder;

const TEST_DURATION: Duration = Duration::from_secs(10);
const REST_DURATION: Duration = TEST_DURATION;

const NUM_THREADS: &[usize] = &[2, 4, 8, 16, 32];

struct State {
    quit: AtomicUsize,
    throughput: AtomicUsize,
}

fn main() {
    println!("num-threads,rayon,cthpool,crossbeam-threadpool");

    for n in NUM_THREADS.iter().copied() {
        print!("{},", n);

        // rayon
        let throughput = {
            let mut throughput = 0;

            for _i in 0..3 {
                let threadpool = RayonBuilder::new()
                    .num_threads(n)
                    .build()
                    .unwrap();

                throughput += testcase(|state| {
                    threadpool.spawn(move || state.update());
                });
            }

            throughput as f32 / 3.0
        };
        print!("{:.3},", throughput);

        // cthpool
        let throughput = {
            let mut throughput = 0;

            for _i in 0..3 {
                let threadpool = CThreadPoolBuilder::new()
                    .num_threads(n)
                    .build();

                throughput += testcase(|state| {
                    threadpool.execute(move || state.update());
                });

                threadpool.join();
            }

            throughput as f32 / 3.0
        };
        print!("{:.3},", throughput);

        // crossbeam-threadpool
        let throughput = {
            let mut throughput = 0;

            for _i in 0..3 {
                let threadpool = CrossbeamBuilder::new()
                    .num_threads(n)
                    .build();

                throughput += testcase(|state| {
                    threadpool.execute(move || state.update());
                });

                threadpool.join();
            }

            throughput as f32 / 3.0
        };
        println!("{:.3}", throughput);
    }
}

fn testcase<F: FnMut(Arc<State>)>(mut f: F) -> usize {
    let state = State::new();

    let finalize_state = Arc::clone(&state);
    thread::spawn(move || {
        thread::sleep(TEST_DURATION);
        finalize_state.finalize();
    });

    while state.is_running() {
        f(Arc::clone(&state));
    }

    thread::sleep(REST_DURATION);
    state.result()
}

impl State {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            quit: AtomicUsize::new(0),
            throughput: AtomicUsize::new(0),
        })
    }

    fn is_running(&self) -> bool {
        self.quit.load(Ordering::SeqCst) == 0
    }

    fn update(&self) {
        if self.is_running() {
            self.throughput.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn finalize(&self) {
        self.quit.store(1, Ordering::SeqCst);
    }

    fn result(&self) -> usize {
        self.throughput.load(Ordering::Relaxed)
    }
}
