/// Collision rate model
///
/// "Collision" = two or more requests in the same batch picking the same key.
/// The preemptive executor tracks accessed keys and must serialize conflicting pairs.
///
/// Model assumptions:
///   - B requests per batch (= concurrent_rqs × total_clients, approximately)
///   - Each request picks a key uniformly at random from K possible keys
///   - "Collision rate" =  expected fraction of requests that share their key
///     with at least one other request in the same batch
///
/// Formula:
///   P(request i collides with ≥1 other) = 1 - (1 - 1/K)^(B-1) ≈ 1 - e^(-(B-1)/K)
///
/// Inverse (K from target rate r):
///   K = ceil(-(B-1) / ln(1 - r))
///
/// Edge cases:
///   r = 0.0  → K = ∞ (use a very large key space in practice)
///   r = 1.0  → K = 1  (all requests hit the same key)

/// Expected per-request collision rate given a key space size and batch size.
///
/// Returns a value in [0.0, 1.0] representing the fraction of requests in a
/// typical batch that will conflict with at least one other request.
pub fn expected_collision_rate(key_space: u64, batch_size: usize) -> f64 {
    if key_space == 0 || batch_size <= 1 {
        return 0.0;
    }
    if key_space == 1 {
        return 1.0;
    }
    let b = (batch_size - 1) as f64;
    let k = key_space as f64;
    1.0 - (1.0 - 1.0 / k).powf(b)
}

/// Key space size required to achieve a target per-request collision rate.
///
/// `batch_size` should be set to `concurrent_rqs × total_clients`.
/// Returns `u64::MAX` for a target rate of 0.0 (not achievable with finite key space).
pub fn key_space_for_collision_rate(target_rate: f64, batch_size: usize) -> u64 {
    assert!(
        (0.0..=1.0).contains(&target_rate),
        "target_rate must be in [0.0, 1.0]"
    );
    assert!(batch_size >= 1, "batch_size must be at least 1");

    if target_rate <= 0.0 {
        return u64::MAX;
    }
    if target_rate >= 1.0 || batch_size == 1 {
        return 1;
    }

    let b = (batch_size - 1) as f64;
    let k = -b / (1.0 - target_rate).ln();
    k.ceil() as u64
}

/// Print a summary table of key space sizes for a set of standard collision rates.
///
/// Useful for planning experiments: call this once at startup when COLLISION_CALC=1.
pub fn print_collision_table(batch_size: usize) {
    println!("Collision rate table (batch_size = {})", batch_size);
    println!(
        "{:>16}  {:>20}  {:>16}",
        "Target rate", "Key space size", "Actual rate"
    );
    println!("{}", "-".repeat(57));

    let target_rates = [0.01, 0.05, 0.10, 0.25, 0.50, 0.75, 0.90, 0.95, 0.99, 1.00];

    for &rate in &target_rates {
        let ks = key_space_for_collision_rate(rate, batch_size);
        let actual = if ks == u64::MAX {
            0.0
        } else {
            expected_collision_rate(ks, batch_size)
        };
        let ks_str = if ks == u64::MAX {
            "∞ (no limit)".to_string()
        } else {
            ks.to_string()
        };
        println!(
            "{:>15.1}%  {:>20}  {:>15.2}%",
            rate * 100.0,
            ks_str,
            actual * 100.0
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_one_key_is_always_collision() {
        assert_eq!(expected_collision_rate(1, 10), 1.0);
    }

    #[test]
    fn rate_huge_keyspace_approaches_zero() {
        let r = expected_collision_rate(1_000_000_000, 100);
        assert!(r < 0.0001, "got {}", r);
    }

    #[test]
    fn inverse_roundtrips() {
        let batch = 200;
        for &target in &[0.10f64, 0.25, 0.50, 0.75, 0.90] {
            let ks = key_space_for_collision_rate(target, batch);
            let actual = expected_collision_rate(ks, batch);
            // actual should be >= target (we rounded up) and within 1% above
            assert!(
                actual >= target - 0.001,
                "target={} actual={}",
                target,
                actual
            );
            assert!(
                actual <= target + 0.01,
                "target={} actual={}",
                target,
                actual
            );
        }
    }

    #[test]
    fn boundary_rate_zero_returns_max() {
        assert_eq!(key_space_for_collision_rate(0.0, 100), u64::MAX);
    }

    #[test]
    fn boundary_rate_one_returns_one() {
        assert_eq!(key_space_for_collision_rate(1.0, 100), 1);
    }
}
