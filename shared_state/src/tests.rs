use std::thread;
use std::ops::Drop;
use std::time::Duration;
use std::sync::{mpsc, Arc, Weak};

use intmap::IntMap;
use rand::prelude::*;
use lock_api::MutexGuard;
use threadpool::ThreadPool;
use parking_lot::{Mutex, RawMutex};

#[cfg_attr(feature = "alloc_jemalloc", global_allocator)]
#[cfg(feature = "alloc_jemalloc")]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[cfg_attr(feature = "alloc_mimalloc", global_allocator)]
#[cfg(feature = "alloc_mimalloc")]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

/* ACTUAL BENCHMARKS                                                          */
////////////////////////////////////////////////////////////////////////////////
#[bench]
fn bench_plain_mutex(b: &mut test::Bencher) {
    //let thread_pool = threadpool::Builder::new()
    //    .build();
    //let h = thread_pool.clone();
    //let mut x = 0;
    //b.iter(move || {
    //    h.execute(move || {
    //        let mut work = Work::new(x);
    //        let mut vec = MEM_POOL.pull(allocate);
    //        work.fill(&mut vec[..TEST_SIZE]);
    //    });
    //    x += 1;
    //    if x % 4 == 0 {
    //        h.join();
    //    }
    //});
    //thread_pool.join();
}

/* TEST IMPLEMENTATION                                                        */
////////////////////////////////////////////////////////////////////////////////

fn work(pool: &ThreadPool) {
    asd
}

fn spawn_sender(tx: mpsc::SyncSender<Message>) {
    thread::spawn(move || {
        let mut rng = thread_rng();
        loop {
            let to: Id = rng.gen_range(0..NUM_CLIENTS);
            let data = ();

            if tx.send(Message { to, data }).is_err() {
                return;
            }
            thread::sleep(SLEEP);
        }
    });
}

fn receiver_thread<S>(live: Weak<S>, rx: mpsc::Receiver<Message>) {
    thread::spawn(move || {
        let mut rng = thread_rng();
        loop {
            let to: Id = rng.gen_range(0..NUM_CLIENTS);
            let data = ();

            if tx.send(Message { to, data }).is_err() {
                return;
            }
            thread::sleep(SLEEP);
        }
    });
}

/* TRAITS AND COSNTS USED BY TESTS                                            */
////////////////////////////////////////////////////////////////////////////////

const NUM_CLIENTS: Id = 16;
const SLEEP: Duration = Duration::from_millis(1);

type Id = usize;
type Data = ();

struct Message {
    to: Id,
    data: Data,
}

trait HandleData {
    fn with<T, F>(&mut self, critical_section: F) -> T
    where
        F: FnMut(Option<&mut Data>) -> T;
}

trait SharedState {
    type HandleData: HandleData;

    fn acquire(&self, who: Id) -> Self::HandleData;
}

/* PLAIN MUTEX                                                                */
////////////////////////////////////////////////////////////////////////////////

struct PlainMutex<'a>(&'a Mutex<IntMap<Data>>);

impl<'a> SharedState for PlainMutex<'a> {
    type HandleData = PlainMutexHandle<'a>;

    fn acquire(&self, who: Id) -> Self::HandleData {
        let handle = self.0.lock();
        PlainMutexHandle { who, handle }
    }
}

struct PlainMutexHandle<'a> {
    who: Id,
    handle: MutexGuard<'a, RawMutex, IntMap<Data>>,
}

impl<'a> HandleData for PlainMutexHandle<'a> {
    fn with<T, F>(&mut self, mut critical_section: F) -> T
    where
        F: FnMut(Option<&mut Data>) -> T
    {
        let data = self.handle.get_mut(self.who as u64);
        (critical_section)(data)
    }
}
