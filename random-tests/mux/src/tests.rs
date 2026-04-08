use std::sync::Arc;

use parking_lot::Mutex;

#[bench]
fn bench_single_mux(b: &mut test::Bencher) {
    let thread_pool = threadpool::Builder::new()
        .build();
    let h = thread_pool.clone();
    let data = Arc::new(new_mutex());
    b.iter(move || {
        let data = Arc::clone(&data);
        h.execute(move || {
            let mut lock = data.lock();
            *lock += 1;
        });
    });
    thread_pool.join();
}

#[bench]
fn bench_array_mux(b: &mut test::Bencher) {
    let thread_pool = threadpool::Builder::new()
        .build();
    let h = thread_pool.clone();
    let cpus = num_cpus::get();
    let data = Arc::new(new_mutexes(cpus));
    let mut next_i = 0;
    b.iter(move || {
        let data = Arc::clone(&data);
        let i = next_i;
        next_i += 1;
        h.execute(move || {
            let mut lock = data[i % cpus].lock();
            *lock += 1;
        });
    });
    thread_pool.join();
}

fn new_mutex() -> Mutex<usize> {
    Mutex::new(0)
}

fn new_mutexes(size: usize) -> Vec<Mutex<usize>> {
    std::iter::repeat_with(new_mutex).take(size).collect()
}
