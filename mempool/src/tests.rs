use lazy_static::lazy_static;
use smallvec::SmallVec;
use object_pool::Pool;

const POOL_CAP: usize = 2048;
const BUF_CAP: usize = 16384;
const TEST_SIZE: usize = 100;

#[cfg_attr(feature = "jemalloc", global_allocator)]
#[cfg(feature = "jemalloc")]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

lazy_static! {
    static ref MEM_POOL: Pool<Vec<u8>> = Pool::new(POOL_CAP, allocate);
}

struct Work {
    x: u8,
}

#[bench]
fn bench_pool(b: &mut test::Bencher) {
    let thread_pool = threadpool::Builder::new()
        .build();
    let h = thread_pool.clone();
    let mut x = 0;
    b.iter(move || {
        h.execute(move || {
            let mut work = Work::new(x);
            let mut vec = MEM_POOL.pull(allocate);
            work.fill(&mut vec[..TEST_SIZE]);
        });
        x += 1;
        if x % 4 == 0 {
            h.join();
        }
    });
    thread_pool.join();
}

#[bench]
fn bench_std(b: &mut test::Bencher) {
    let thread_pool = threadpool::Builder::new()
        .build();
    let h = thread_pool.clone();
    let mut x = 0;
    b.iter(move || {
        h.execute(move || {
            let mut work = Work::new(x);
            let mut vec = allocate();
            work.fill(&mut vec[..TEST_SIZE]);
        });
        x += 1;
        if x % 4 == 0 {
            h.join();
        }
    });
    thread_pool.join();
}

#[bench]
fn bench_smallvec(b: &mut test::Bencher) {
    let thread_pool = threadpool::Builder::new()
        .build();
    let h = thread_pool.clone();
    let mut x = 0;
    b.iter(move || {
        h.execute(move || {
            let mut work = Work::new(x);
            let mut vec = allocate_smallvec();
            work.fill(&mut vec[..TEST_SIZE]);
        });
        x += 1;
        if x % 4 == 0 {
            h.join();
        }
    });
    thread_pool.join();
}

fn allocate() -> Vec<u8> {
    Vec::with_capacity(BUF_CAP)
}

fn allocate_smallvec() -> SmallVec<[u8; BUF_CAP]> {
    SmallVec::new()
}

impl Work {
    fn new(x: u8) -> Work {
        Work { x }
    }

    fn fill(&mut self, s: &mut [u8]) {
        for x in s.iter_mut() {
            *x = self.x;
        }
    }
}
