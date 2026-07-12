//! M4: state-transfer / checkpoint / reconfiguration crash scenarios.
//!
//! What's implemented here is the **recovery / catch-up** sub-case, which needs no Atlas
//! change: a replica crashes, comes back with an empty db (`restart_amnesia`, FU-6), and
//! must catch up to the cluster's current sequence via Atlas's peer-driven transfer to
//! participate again. Correctness is checked through the echo app's `op_count` (part of the
//! monolithic state): a node that failed to catch up would reply with a *different*
//! `op_count`, which the byte-gate reply observer would flag as a conflict.
//!
//! The **checkpoint-crash** and **state-snapshot-crash** sub-cases are NOT here: forcing a
//! checkpoint (so a snapshot state-transfer, rather than log replay, is exercised) requires
//! turning febft/Atlas's `CHECKPOINT_PERIOD` const into a test-settable override (F7 — an
//! Atlas source change). The **reconfiguration-coordinator crash** (between `LockedQC` and
//! commit) needs a reconfiguration-message content predicate. Both are tracked as follow-ups.
//!
//! febft-only (uses `restart_amnesia`, gated on `RestartableBackend`). Run with `cargo nextest run`.

mod common;

use std::time::Duration;

use correctness_testing::oracle::reply_log::record_replies;
use correctness_testing::oracle::{ClientPool, ReplyLog};
use correctness_testing::{ClusterHarness, Febft, NodeId};

/// A crashed replica rejoins with an empty db and must catch up: after recovery it executes
/// current operations and its replies AGREE with the always-live replicas (same `op_count`),
/// proving it recovered the committed state — not started from scratch.
#[test]
fn recovered_node_catches_up_and_agrees() {
    common::init_tracing();

    let mut harness = ClusterHarness::<Febft>::new(4).expect("cluster bootstraps");
    let mut client = harness.new_client().expect("client bootstraps");

    // Build up committed state (op_count climbs) before the fault.
    for i in 0..6u64 {
        assert_eq!(harness.submit_ordered(&mut client, i, 10 + i).unwrap().value, 10 + i);
    }

    // Crash a replica and bring it back with a fresh empty db.
    let node = NodeId::from(3u32);
    harness.crash(node).expect("crash node 3");
    harness
        .restart_amnesia(node)
        .expect("amnesia-restarted node should rejoin");

    // Observe replies ONLY from here on, so we isolate post-recovery behavior.
    let replies = ReplyLog::new();
    harness.set_observer(record_replies(replies.clone()));

    // Drive post-recovery load. The recovered node must catch up enough to execute these
    // and reply consistently with everyone else.
    let pool = ClientPool::from_client(harness.new_client().unwrap(), 16).expect("pool");
    pool.submit_n(6, 100).expect("post-recovery submit");
    pool.assert_all_completed(Duration::from_secs(30))
        .expect("post-recovery requests complete");

    // Give the recovered node a beat to have its replies observed.
    std::thread::sleep(Duration::from_millis(500));

    assert!(
        replies.total_replies() > 0,
        "reply observer must have seen post-recovery replies"
    );
    // No two replicas disagreed on any result — in particular, the recovered node's
    // op_count matches the others, so it caught up to the committed state.
    assert!(
        !replies.has_conflict(),
        "recovered node produced a divergent result (failed to catch up?): {:?}",
        replies.conflicts()
    );
    // And the recovered node actually participated in executing post-recovery ops.
    assert!(
        replies.repliers().contains(&node),
        "the recovered node ({node:?}) should be replying to post-recovery requests \
         (it did not rejoin execution); repliers were {:?}",
        replies.repliers()
    );
}
