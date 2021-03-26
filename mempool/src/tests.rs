use lazy_static::lazy_static;
use object_pool::Pool;

const POOL_CAP: usize = 2048;
const BUF_CAP: usize = 8192;

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
    b.iter(move || h.execute(|| {
        let mut work = Work::new();
        let mut vec = MEM_POOL.pull(allocate);
        work.fill(&mut vec);
    }));
    thread_pool.join();
}

#[bench]
fn bench_std(b: &mut test::Bencher) {
    let thread_pool = threadpool::Builder::new()
        .build();
    let h = thread_pool.clone();
    b.iter(move || h.execute(|| {
        let mut work = Work::new();
        let mut vec = std::hint::black_box(allocate());
        work.fill(&mut vec);
    }));
    thread_pool.join();
}

fn allocate() -> Vec<u8> {
    Vec::with_capacity(BUF_CAP)
}

impl Work {
    fn new() -> Work {
        Work { x: 0 }
    }

    fn fill(&mut self, s: &mut [u8]) {
        for x in s.iter_mut() {
            *x = self.x;
        }
        self.x += 1;
    }
}
