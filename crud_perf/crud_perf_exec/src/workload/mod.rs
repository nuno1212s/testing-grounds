use std::time::Duration;

use rand::distributions::Distribution;
use rand::{Rng, SeedableRng};
use rand_distr::{Uniform, WeightedIndex, Zipf};
use rand_xoshiro::SplitMix64;

use crate::knobs::VALUE_LEN;
use crate::serialize::{CRUDRequestType, Key};

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
        op_cost: Duration,
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
            expensive_sleep: op_cost,
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
                // 20% of ops are expensive; seeded so every run sees the same stream.
                if self.rng.gen_range(0..10u8) < 2 {
                    self.expensive_sleep
                } else {
                    Duration::ZERO
                }
            }
            WorkloadType::Adversarial => {
                // 5% of ops are very long (10x).
                if self.rng.gen_range(0..20u8) == 0 {
                    self.expensive_sleep * 10
                } else {
                    Duration::ZERO
                }
            }
            WorkloadType::CRUDProportional => Duration::ZERO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(kind: WorkloadType, seed: u64, n: usize) -> Vec<(Vec<u8>, Duration, bool)> {
        let mut generator = WorkloadGenerator::new(
            seed,
            1_000_000,
            kind,
            KeyDistributionKind::Uniform,
            0.1,
            Duration::from_micros(250),
            70,
            15,
            10,
            5,
        );

        (0..n)
            .map(|_| {
                let op = generator.next_op(false);
                let bytes = bincode::serde::encode_to_vec(&op.request_type, bincode::config::standard())
                    .unwrap();
                (bytes, op.time_delay, op.is_ordered)
            })
            .collect()
    }

    /// Mixed/adversarial delays once came from an unseeded RNG, so a seed gave a different stream.
    #[test]
    fn every_workload_is_reproducible_from_its_seed() {
        for kind in [
            WorkloadType::UniformCheap,
            WorkloadType::UniformExpensive,
            WorkloadType::Mixed8020,
            WorkloadType::Adversarial,
            WorkloadType::CRUDProportional,
        ] {
            assert_eq!(
                stream(kind.clone(), 7919, 5_000),
                stream(kind.clone(), 7919, 5_000),
                "{kind:?} is not deterministic"
            );
        }
    }

    #[test]
    fn different_seeds_give_different_streams() {
        assert_ne!(
            stream(WorkloadType::Mixed8020, 1, 1_000),
            stream(WorkloadType::Mixed8020, 2, 1_000)
        );
    }

    #[test]
    fn mixed_workload_actually_mixes_costs() {
        let costs: std::collections::HashSet<Duration> = stream(WorkloadType::Mixed8020, 3, 2_000)
            .into_iter()
            .map(|(_, d, _)| d)
            .collect();

        assert_eq!(costs.len(), 2, "expected exactly {{0, cost}}, got {costs:?}");
    }
}
