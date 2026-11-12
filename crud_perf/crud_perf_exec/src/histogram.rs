//! Lock-free log-linear latency histogram (HdrHistogram layout, 6 sub-bucket bits, <1.6%
//! error). `suite/lib/hist.py` mirrors the bucket arithmetic; the golden-vector test pins both.

use std::sync::atomic::{AtomicU64, Ordering};

pub const SUB_BITS: u32 = 6;
const SUB_COUNT: usize = 1 << SUB_BITS;
/// One exact range, then one range per remaining bit of a u64.
pub const BUCKETS: usize = (64 - SUB_BITS as usize + 1) * SUB_COUNT;

/// Index of the bucket holding `value`.
pub fn bucket_index(value: u64) -> usize {
    if value < SUB_COUNT as u64 {
        return value as usize;
    }

    let msb = 63 - value.leading_zeros();
    let shift = msb - SUB_BITS;

    (((shift as usize) + 1) << SUB_BITS) + ((value >> shift) as usize & (SUB_COUNT - 1))
}

/// Smallest value in bucket `index` (only tests use it here; the runner decodes in Python).
#[allow(dead_code)]
pub fn bucket_lower(index: usize) -> u64 {
    if index < SUB_COUNT {
        return index as u64;
    }

    let octave = index >> SUB_BITS;
    let sub = (index & (SUB_COUNT - 1)) as u64;

    (SUB_COUNT as u64 + sub) << (octave - 1)
}

pub struct LatencyHistogram {
    buckets: Box<[AtomicU64]>,
    count: AtomicU64,
    sum: AtomicU64,
    max: AtomicU64,
}

impl LatencyHistogram {
    pub fn new() -> Self {
        Self {
            buckets: (0..BUCKETS).map(|_| AtomicU64::new(0)).collect(),
            count: AtomicU64::new(0),
            sum: AtomicU64::new(0),
            max: AtomicU64::new(0),
        }
    }

    /// Record one sample in nanoseconds; wait-free.
    pub fn record(&self, value_ns: u64) {
        self.buckets[bucket_index(value_ns)].fetch_add(1, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        self.sum.fetch_add(value_ns, Ordering::Relaxed);
        self.max.fetch_max(value_ns, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    /// `{"count":..,"sum_ns":..,"max_ns":..,"buckets":{"<index>":<count>,..}}`, non-empty only.
    pub fn to_json(&self) -> String {
        let mut out = format!(
            "{{\"count\":{},\"sum_ns\":{},\"max_ns\":{},\"buckets\":{{",
            self.count.load(Ordering::Relaxed),
            self.sum.load(Ordering::Relaxed),
            self.max.load(Ordering::Relaxed),
        );

        let mut first = true;
        for (index, bucket) in self.buckets.iter().enumerate() {
            let n = bucket.load(Ordering::Relaxed);
            if n == 0 {
                continue;
            }
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str(&format!("\"{index}\":{n}"));
        }

        out.push_str("}}");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_values_are_exact() {
        for v in 0..SUB_COUNT as u64 {
            assert_eq!(bucket_lower(bucket_index(v)), v);
        }
    }

    #[test]
    fn index_and_lower_bound_agree_across_the_range() {
        let mut values: Vec<u64> = (0..100_000).collect();
        let mut v = 100_000u64;
        while v < u64::MAX / 3 {
            values.push(v);
            values.push(v + 1);
            v = v * 3 / 2 + 7;
        }
        values.push(u64::MAX);

        for v in values {
            let i = bucket_index(v);
            assert!(i < BUCKETS, "index {i} out of range for {v}");

            let lo = bucket_lower(i);
            assert!(lo <= v, "lower bound {lo} above value {v}");

            if i + 1 < BUCKETS {
                assert!(v < bucket_lower(i + 1), "value {v} escapes bucket {i}");
            }

            if v >= SUB_COUNT as u64 {
                assert!(
                    ((v - lo) as f64) / (v as f64) < 1.0 / 64.0,
                    "error too large at {v}"
                );
            }
        }
    }

    /// Same literals as `suite/tests/test_suite.py`.
    #[test]
    fn golden_vectors_shared_with_the_python_decoder() {
        const GOLDEN: [(u64, usize, u64); 21] = [
            (0, 0, 0),
            (1, 1, 1),
            (63, 63, 63),
            (64, 64, 64),
            (65, 65, 65),
            (127, 127, 127),
            (128, 128, 128),
            (129, 128, 128),
            (130, 129, 130),
            (255, 191, 254),
            (256, 192, 256),
            (257, 192, 256),
            (1000, 317, 1000),
            (65535, 703, 65024),
            (65536, 704, 65536),
            (1_000_000, 954, 999_424),
            (123_456_789, 1397, 122_683_392),
            (1_099_511_627_776, 2240, 1_099_511_627_776),
            (4_611_686_018_427_387_904, 3648, 4_611_686_018_427_387_904),
            (9_223_372_036_854_775_808, 3712, 9_223_372_036_854_775_808),
            (18_446_744_073_709_551_615, 3775, 18_302_628_885_633_695_744),
        ];

        for (value, index, lower) in GOLDEN {
            assert_eq!(bucket_index(value), index, "index of {value}");
            assert_eq!(bucket_lower(index), lower, "lower bound of bucket {index}");
        }
        assert_eq!(BUCKETS, 3776);
    }

    #[test]
    fn indices_are_monotonic() {
        let mut last = 0;
        for v in (0..2_000_000u64).step_by(7) {
            let i = bucket_index(v);
            assert!(i >= last);
            last = i;
        }
    }

    #[test]
    fn json_lists_only_populated_buckets() {
        let h = LatencyHistogram::new();
        h.record(10);
        h.record(10);
        h.record(1_000_000);

        let json = h.to_json();
        assert!(json.starts_with("{\"count\":3,\"sum_ns\":1000020,\"max_ns\":1000000,"));
        assert!(json.contains("\"10\":2"));
        assert_eq!(h.count(), 3);
    }
}
