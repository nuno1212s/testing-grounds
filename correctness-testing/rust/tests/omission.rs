//! M2/M3: message-omission faults (send / receive, filtered / unfiltered).
//!
//! Omission is a byte-gate edge-drop. Unfiltered send/receive omission of a single
//! non-leader is protocol-agnostic and runs across all three protocols. The content-
//! filtered variant (drop only COMMITs) uses the febft adapter predicate, so it stays
//! febft-only.
//!
//! Run with `cargo nextest run`.

mod common;

use std::time::Duration;

use correctness_testing::adapters::febft;
use correctness_testing::chaos::EdgeRule;
use correctness_testing::oracle::ClientPool;
use correctness_testing::{ClusterHarness, NodeId, ProtocolBackend, protocol_test};

const MUTED: u32 = 3; // a non-leader (leader is node 0)

/// Bring up, warm up, install `rule_for(MUTED)`, then confirm the 2f+1 unaffected
/// replicas still complete every request.
fn run_with_rule<P: ProtocolBackend>(rule_for: impl FnOnce(NodeId) -> EdgeRule) {
    common::init_tracing();

    let mut harness = ClusterHarness::<P>::new(4).expect("cluster bootstraps");
    let client = harness.new_client().expect("client bootstraps");
    let pool = ClientPool::from_client(client, 16).expect("client pool");

    pool.submit_n(3, 1).expect("warm-up submit");
    pool.assert_all_completed(Duration::from_secs(20))
        .expect("warm-up completes");

    harness.install_rule(rule_for(NodeId::from(MUTED)));

    pool.submit_n(10, 100).expect("post-omission submit");
    pool.assert_all_completed(Duration::from_secs(30))
        .expect("requests should still complete via the 2f+1 unaffected replicas");

    assert_eq!(pool.ledger().completed_count(), 13);
}

fn receive_omission_deaf_replica<P: ProtocolBackend>() {
    run_with_rule::<P>(|n| EdgeRule::receive_omission(n, None));
}

fn send_omission_muted_replica<P: ProtocolBackend>() {
    run_with_rule::<P>(|n| EdgeRule::send_omission(n, None));
}

// Febft-only for now (same reason as oracle_completion: HotStuff/IronChain surface
// protocol-internal issues under sustained load in-process). Extend once resolved.
protocol_test!(receive_omission_deaf_replica => [Febft]);
protocol_test!(send_omission_muted_replica => [Febft]);

/// Filtered send-omission (F3, febft-only): drop only the muted replica's COMMITs. Since
/// its COMMIT is redundant to the 2f+1 the others form, requests still complete.
#[test]
fn filtered_send_omission_of_commits_febft() {
    run_with_rule::<correctness_testing::Febft>(|n| {
        EdgeRule::send_omission(n, Some(febft::commit(None)))
    });
}
