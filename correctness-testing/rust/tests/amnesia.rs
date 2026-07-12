//! M2/FU-6: amnesia / equivocation safety.
//!
//! Safety oracle: a byte-gate vote observer (`VoteLog` + `febft::record_votes`) records
//! every PREPARE/COMMIT each node casts, keyed by `(node, seq, phase)` with the batch
//! digest. Equivocation = two *different* digests for the same key — meaningful only with
//! DISTINCT per-node keys (F6), which `identity.rs` synthesizes.
//!
//! Run with `cargo nextest run`.

mod common;

use std::time::Duration;

use correctness_testing::adapters::febft;
use correctness_testing::oracle::{ClientPool, VoteLog};
use correctness_testing::{ClusterHarness, Febft, NodeId};

/// Negative control: with no faults and distinct keys, the observer sees real votes but
/// detects NO equivocation. Proves the oracle isn't vacuous and doesn't false-positive.
#[test]
fn amnesia_negative_control_no_equivocation() {
    common::init_tracing();

    let mut harness = ClusterHarness::<Febft>::new(4).expect("cluster bootstraps");

    let votes = VoteLog::new();
    harness.set_observer(febft::record_votes(votes.clone()));

    let client = harness.new_client().expect("client bootstraps");
    let pool = ClientPool::from_client(client, 16).expect("client pool");

    pool.submit_n(20, 1).expect("submit ordered requests");
    pool.assert_all_completed(Duration::from_secs(30))
        .expect("all complete");

    assert!(
        votes.total_votes() > 0,
        "the gate vote observer must actually record consensus votes"
    );
    assert!(
        !votes.has_equivocation(),
        "no equivocation expected under fault-free operation with distinct keys (F6); found: {:?}",
        votes.equivocations()
    );
}

/// A replica crashes, loses its state, and rejoins with an empty db (`restart_amnesia`,
/// FU-6). It must catch up via Atlas's peer-driven recovery **without equivocating** — it
/// must never cast a vote that conflicts with one it (or another replica) already cast for
/// the same (seq, phase). This exercises the full crash → amnesia-restart → recover →
/// equivocation-oracle pipeline.
#[test]
fn amnesia_restart_recovers_without_equivocation() {
    common::init_tracing();

    let mut harness = ClusterHarness::<Febft>::new(4).expect("cluster bootstraps");

    let votes = VoteLog::new();
    harness.set_observer(febft::record_votes(votes.clone()));

    let client = harness.new_client().expect("client bootstraps");
    let pool = ClientPool::from_client(client, 16).expect("client pool");

    // Drive load so the target replica casts (recorded) votes.
    pool.submit_n(5, 1).expect("pre-crash submit");
    pool.assert_all_completed(Duration::from_secs(20))
        .expect("pre-crash completes");

    // Crash a replica and bring it back with a fresh empty db (amnesia).
    let node = NodeId::from(3u32);
    harness.crash(node).expect("crash node 3");
    harness
        .restart_amnesia(node)
        .expect("amnesia-restarted node should rejoin");

    // More load: the recovered node participates again as it catches up.
    pool.submit_n(5, 100).expect("post-recovery submit");
    pool.assert_all_completed(Duration::from_secs(30))
        .expect("cluster keeps completing requests after the node rejoins");

    assert!(votes.total_votes() > 0, "votes must have been observed");
    assert!(
        !votes.has_equivocation(),
        "an amnesia-recovered node must not equivocate; found: {:?}",
        votes.equivocations()
    );
}

/// The DANGEROUS amnesia double-vote (documented known gap — "demonstrate, not close").
///
/// To force an actual equivocation, the amnesia-recovered node must *re-vote* for a
/// sequence it already voted on before crashing — which requires that sequence to still be
/// undecided when it rejoins, i.e. a view change on the in-flight sequence. That recovery
/// path does not yet trigger in-process (tracked as the febft idle-leader-crash / view-
/// change gap). The restart mechanism it needs (`restart_amnesia`, FU-6) is now
/// implemented and exercised by `amnesia_restart_recovers_without_equivocation` above; the
/// remaining blocker is the view-change trigger.
#[test]
#[ignore = "known gap: forcing the double-vote needs a view change on the in-flight seq (febft view-change gap); restart_amnesia (FU-6) is done"]
fn amnesia_double_vote_known_gap() {
    // Placeholder — see the doc comment. The vote oracle and restart_amnesia are ready;
    // the missing piece is the view-change that would make the recovered node re-vote.
}
