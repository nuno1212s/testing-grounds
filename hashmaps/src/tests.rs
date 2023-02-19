use std::hash::{BuildHasher, BuildHasherDefault};
use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::default::Default;

use fxhash::FxHasher64;
use seahash::SeaHasher;
use twox_hash::XxHash64;
use metrohash::{MetroHash64, MetroHash128};
use highway::{AvxHash, SseHash, PortableHash};

#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[bench]
fn bench_hashmap_std(b: &mut test::Bencher) {
    bench_hashmap(RandomState::new(), b);
}

#[bench]
fn bench_hashmap_seahash(b: &mut test::Bencher) {
    bench_hashmap(BuildHasherDefault::<SeaHasher>::default(), b);
}

#[bench]
fn bench_hashmap_xxhash(b: &mut test::Bencher) {
    bench_hashmap(BuildHasherDefault::<XxHash64>::default(), b);
}

#[bench]
fn bench_hashmap_fxhash(b: &mut test::Bencher) {
    bench_hashmap(BuildHasherDefault::<FxHasher64>::default(), b);
}

#[bench]
fn bench_hashmap_metrohash64(b: &mut test::Bencher) {
    bench_hashmap(BuildHasherDefault::<MetroHash64>::default(), b);
}

#[bench]
fn bench_hashmap_metrohash128(b: &mut test::Bencher) {
    bench_hashmap(BuildHasherDefault::<MetroHash128>::default(), b);
}

#[bench]
fn bench_hashmap_hw_portable(b: &mut test::Bencher) {
    bench_hashmap(BuildHasherDefault::<PortableHash>::default(), b);
}

#[bench]
fn bench_hashmap_hw_avx(b: &mut test::Bencher) {
    bench_hashmap(BuildHasherDefault::<AvxHash>::default(), b);
}

#[bench]
fn bench_hashmap_hw_sse(b: &mut test::Bencher) {
    bench_hashmap(BuildHasherDefault::<SseHash>::default(), b);
}

#[inline(always)]
fn bench_hashmap<S: BuildHasher>(build_hasher: S, b: &mut test::Bencher) {
    let mut work = Work::new();
    let mut map = HashMap::with_capacity_and_hasher(8, build_hasher);
    b.iter(move || {
        let digest = work.digest();
        let _ = map.insert(digest, ());
        let _ = map.remove(&digest);
    });
}

#[derive(Copy, Clone)]
struct Work {
    x: u8,
}

impl Work {
    const fn new() -> Work {
        Work { x: 0 }
    }

    fn digest(&mut self) -> Digest {
        let mut digest = [0; DIGEST_SIZE];

        for x in digest.iter_mut() {
            *x = self.x;
        }

        self.x += 1;
        digest
    }
}

const DIGEST_SIZE: usize = 256 / 8;

type Digest = [u8; DIGEST_SIZE];
