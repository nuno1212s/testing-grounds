use std::time::Duration;

use rand::SeedableRng;
use rand::distributions::Distribution;
use rand_distr::{Uniform, WeightedIndex, Zipf};
use rand_xoshiro::SplitMix64;

use crate::serialize::{CRUDRequestType, Key};

const VALUE_LEN: usize = 64;

#[derive(Debug, Clone)]
pub enum WorkloadType {
    UniformCheap,
    UniformExpensive,
    Mixed8020,
    Adversarial,
    CRUDProportional,
}

impl WorkloadType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "uniform_cheap" => Self::UniformCheap,
            "uniform_expensive" => Self::UniformExpensive,
            "mixed_8020" => Self::Mixed8020,
            "adversarial" => Self::Adversarial,
            "crud_proportional" => Self::CRUDProportional,
            other => panic!("Unknown workload type: {}", other),
        }
    }
}

#[derive(Debug, Clone)]
pub enum KeyDistributionKind {
    Uniform,
    Zipf,
}

impl KeyDistributionKind {
    pub fn from_str(s: &str) -> Self {
        match s {
            "uniform" => Self::Uniform,
            "zipf" => Self::Zipf,
            other => panic!("Unknown key distribution: {}", other),
        }
    }
}

pub struct GeneratedOp {
    pub request_type: CRUDRequestType,
    pub time_delay: Duration,
    pub is_ordered: bool,
}

pub struct WorkloadGenerator {
    key_space: u64,
    distribution: KeyDistributionKind,
    uniform_dist: Uniform<u64>,
    zipf_dist: Option<Zipf<f64>>,
    op_weights: WeightedIndex<u32>,
    expensive_sleep: Duration,
    workload_type: WorkloadType,
    rng: SplitMix64,
    payload: Vec<u8>,
}

impl WorkloadGenerator {
    pub fn new(
        client_seed: u64,
        key_space: u64,
        workload_type: WorkloadType,
        distribution: KeyDistributionKind,
        zipf_constant: f64,
        expensive_op_sleep_ms: u64,
        read_ratio: u32,
        create_ratio: u32,
        update_ratio: u32,
        delete_ratio: u32,
    ) -> Self {
        let weights = [read_ratio, create_ratio, update_ratio, delete_ratio];
        let op_weights =
            WeightedIndex::new(weights).expect("Invalid operation weight distribution");

        let uniform_dist = Uniform::new(0, key_space);
        let zipf_dist = match distribution {
            KeyDistributionKind::Zipf => {
                Some(Zipf::new(key_space, zipf_constant).expect("Invalid Zipf parameters"))
            }
            KeyDistributionKind::Uniform => None,
        };

        Self {
            key_space,
            distribution,
            uniform_dist,
            zipf_dist,
            op_weights,
            expensive_sleep: Duration::from_millis(expensive_op_sleep_ms),
            workload_type,
            rng: SplitMix64::seed_from_u64(client_seed),
            payload: vec![0u8; VALUE_LEN],
        }
    }

    pub fn next_op(&mut self, force_ordered: bool) -> GeneratedOp {
        let key = self.next_key();
        let op_index = self.op_weights.sample(&mut self.rng);

        let time_delay = self.sample_delay(op_index);

        let (request_type, is_write) = match op_index {
            0 => (CRUDRequestType::Read { key }, false),
            1 => (
                CRUDRequestType::Create {
                    key,
                    data: self.payload.clone(),
                },
                true,
            ),
            2 => (
                CRUDRequestType::Update {
                    key,
                    data: self.payload.clone(),
                },
                true,
            ),
            3 => (CRUDRequestType::Delete { key }, true),
            _ => unreachable!(),
        };

        let is_ordered = force_ordered || is_write;

        GeneratedOp {
            request_type,
            time_delay,
            is_ordered,
        }
    }

    fn next_key(&mut self) -> Key {
        match self.distribution {
            KeyDistributionKind::Uniform => {
                Key::from_index(self.uniform_dist.sample(&mut self.rng))
            }
            KeyDistributionKind::Zipf => {
                // Zipf is 1-indexed; subtract 1 to get 0-based index
                let idx = self.zipf_dist.as_ref().unwrap().sample(&mut self.rng) as u64 - 1;
                Key::from_index(idx.min(self.key_space - 1))
            }
        }
    }

    fn sample_delay(&mut self, op_index: usize) -> Duration {
        match self.workload_type {
            WorkloadType::UniformCheap => Duration::ZERO,
            WorkloadType::UniformExpensive => self.expensive_sleep,
            WorkloadType::Mixed8020 => {
                // 20% of ops are expensive
                if fastrand::u8(0..10) < 2 {
                    self.expensive_sleep
                } else {
                    Duration::ZERO
                }
            }
            WorkloadType::Adversarial => {
                // 5% of ops are very long (10x expensive sleep)
                if fastrand::u8(0..20) == 0 {
                    self.expensive_sleep * 10
                } else {
                    Duration::ZERO
                }
            }
            WorkloadType::CRUDProportional => Duration::ZERO,
        }
    }
}
