use rand::{distributions::DistString, Rng};
use rand_core::SeedableRng;
use rand_distr::{Alphanumeric, Distribution, Standard, Uniform, WeightedIndex, Zipf};
use rand_xoshiro::{self, SplitMix64};
use std::collections::HashMap;
use std::iter::once;
use std::sync::Arc;
use crate::serialize::MicrobenchmarkData;

const SECONDARY_KEY_LEN: usize = 16;
const VALUE_LEN: usize = 64;
const HASHMAP_LEN: usize = 10;

// for more "randomness" in the distribution this should be between  ]0.0,0.24[
const ZIPF_CONSTANT: f64 = 0.1;

pub const NUM_KEYS: usize = 128000;
const INSERT_OPS: u32 = 0;
const READ_OPS: u32 = 20;
const REMOVE_OPS: u32 = 0;
const UPDATE_OPS: u32 = 1;

#[derive(Debug)]
pub struct Generator {
    pool: Arc<[Arc<str>]>,
    distribution: Zipf<f64>,
    size: u64,
}

impl Generator {
    pub fn new(pool: Arc<[Arc<str>]>, size: u64) -> Self {
        let distribution = Zipf::new(size, ZIPF_CONSTANT).expect("fail");
        Self {
            pool,
            distribution,
            size,
        }
    }

    //get a random Zipfian distributed key, if the zipfian constant is 0, the distribution will be uniform, constants > ~.25 will start to behave more like an exponential distribution.
    pub fn get_key_zipf<R: Rng + ?Sized>(&self, rng: &mut R) -> Arc<str> {
        // the distribution generates integers starting at 1 while the indexes of the Pool start at 0.
        let index = (self.distribution.sample(rng) - 1.0) as usize;
        let key = self.pool.get(index);

        key.unwrap().clone()
    }

    //get a random, uniformly distributed ke
    pub fn get_rand_key<R: Rng + ?Sized>(&self, rng: &mut R) -> Arc<str> {
        self.pool
            .get(Uniform::new(0, self.size).sample(rng) as usize)
            .unwrap()
            .clone()
    }

    pub fn get(&self, idx: usize) -> Option<Arc<str>> {
        if let Some(res) = self.pool.get(idx) {
            Some(res.clone())
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct Entry {
    key: String,
    value: String,
}

impl Entry {
    pub fn new(key: String, value: String) -> Self {
        Self { key, value }
    }

    pub fn iter(&self) -> impl Iterator<Item=&str> {
        once(self.key.as_str()).chain(once(self.value.as_str()))
    }
}

impl Distribution<Entry> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Entry {
        let key = Alphanumeric.sample_string(rng, SECONDARY_KEY_LEN);
        let value = Alphanumeric.sample_string(rng, VALUE_LEN);
        Entry::new(key, value)
    }
}

pub fn generate_key_pool(num_keys: usize) -> (Arc<[Arc<str>]>, Arc<str>) {
    let mut pool = Vec::new();
    let mut rand = SplitMix64::seed_from_u64(160120241634);

    for _ in 0..num_keys {
        let _ = pool
            .push(Arc::from(Alphanumeric
                .sample_string(&mut rand, MicrobenchmarkData::KEY_SIZE)));
    }


    (Arc::from(pool), Arc::from(Alphanumeric.sample_string(&mut rand, MicrobenchmarkData::VALUE_SIZE)))
}

pub fn generate_kv_pairs<R: Rng>(rand: &mut R) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();

    for _ in 0..HASHMAP_LEN {
        let entry: Entry = rand.gen();
        map.insert(entry.key, entry.value);
    }

    map
}

pub struct OpStandard {
    weighted_index: WeightedIndex<u32>,
}

#[derive(Debug, Clone)]
pub enum Operation {
    Read,
    Insert,
    Remove,
    Update,
}

impl Default for OpStandard {
    fn default() -> Self {
        let weights = [READ_OPS, INSERT_OPS, REMOVE_OPS, UPDATE_OPS];

        Self {
            weighted_index: WeightedIndex::new(weights)
                .expect("error creating weighted distribution"),
        }
    }
}

impl Distribution<Operation> for OpStandard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Operation {
        match self.weighted_index.sample(rng) {
            0 => Operation::Read,
            1 => Operation::Insert,
            2 => Operation::Remove,
            3 => Operation::Update,
            _ => panic!("INVALID OPERATION"),
        }
    }
}
