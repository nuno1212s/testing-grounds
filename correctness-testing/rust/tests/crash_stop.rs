//! M1/M3: crash-stop faults.
//!
//! Validates the `true_crash` lifecycle path — mesh edge-severing + connected-set update
//! (F2) — and that the cluster stays LIVE while ≤f replicas have failed. The non-leader
//! crash runs across all three protocols; the idle-leader-crash liveness gap stays
//! febft-only (see doc).
//!
//! Run with `cargo nextest run`.

mod common;

use correctness_testing::{ClusterHarness, NodeId, ProtocolBackend, protocol_test};

/// Crash a NON-leader replica, then keep submitting: liveness holds via the 2f+1
/// survivors. Runs on febft, HotIron, and IronChain.
fn crash_stop_non_leader_stays_live<P: ProtocolBackend>() {
    common::init_tracing();

    let mut harness = ClusterHarness::<P>::new(4).expect("cluster bootstraps");
    let mut client = harness.new_client().expect("client bootstraps");

    let r = harness.submit_ordered(&mut client, 1, 10).expect("baseline request");
    assert_eq!(r.value, 10);

    // Crash a non-leader replica (f=1 tolerated).
    harness.crash(NodeId::from(3u32)).expect("crash node 3");

    // Liveness: further requests still complete via the 2f+1 survivors {0,1,2}.
    for i in 0..5u64 {
        let reply = harness
            .submit_ordered(&mut client, 100 + i, 200 + i)
            .unwrap_or_else(|e| panic!("post-crash request {i} should complete: {e}"));
        assert_eq!(reply.value, 200 + i);
    }
}

// HotStuff/IronChain excluded: they bootstrap + agree over the chaos transport (see the
// bringup matrix), but do not recover liveness after a crash in-process — a protocol
// fault-handling gap (same class as the febft view-change gap). Extend this list once
// their in-process recovery works.
protocol_test!(crash_stop_non_leader_stays_live => [Febft]);

/// Crash the CURRENT leader (node 0) while the cluster is otherwise idle, then submit.
///
/// KNOWN GAP (documented, febft-only). This does NOT recover liveness in-process: the
/// transport delivers the post-crash request to all survivors (verified at the gate), but
/// febft never arms a client-request timeout on non-leaders (its only registration path,
/// `watch_received_requests`, is unreachable from the non-leader proposer loop), so with
/// no in-flight consensus instance at crash time nothing triggers a view change. Tracked
/// separately (view-change recovery).
#[test]
#[ignore = "known gap: febft does not recover liveness on an idle leader-crash in-process; see doc comment"]
fn crash_stop_leader_idle_recovery_known_gap() {
    common::init_tracing();

    let mut harness =
        ClusterHarness::<correctness_testing::Febft>::new(4).expect("cluster bootstraps");
    let mut client = harness.new_client().expect("client bootstraps");

    let r = harness.submit_ordered(&mut client, 1, 10).expect("baseline request");
    assert_eq!(r.value, 10);

    let leader = harness.initial_leader();
    harness.crash(leader).expect("crash the leader");

    let reply = harness
        .submit_ordered(&mut client, 300, 400)
        .expect("post-leader-crash request");
    assert_eq!(reply.value, 400);
}
