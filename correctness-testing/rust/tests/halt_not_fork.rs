//! M5: f+1 crash — the cluster HALTS but does not FORK.
//!
//! With n=4/f=1, crashing f+1=2 replicas leaves only 2f=2 survivors, below the 2f+1 quorum.
//! The safety/liveness contract:
//!   (i)   requests that already completed (reached quorum pre-crash) stay completed;
//!   (ii)  requests submitted after the crash do NOT complete — liveness is correctly lost
//!         with no reachable quorum (the "halt");
//!   (iii) no execution-level fork — no two replicas ever report different results for the
//!         same client operation (checked via the byte-gate reply observer).
//!
//! febft-only. Run with `cargo nextest run`.

mod common;

use std::time::Duration;

use correctness_testing::oracle::reply_log::record_replies;
use correctness_testing::oracle::{ClientPool, ReplyLog};
use correctness_testing::{ClusterHarness, Febft, NodeId};

#[test]
fn f_plus_one_crash_halts_without_forking() {
    common::init_tracing();

    let mut harness = ClusterHarness::<Febft>::new(4).expect("cluster bootstraps");

    // Watch every replica→client reply for divergence (fork detection).
    let replies = ReplyLog::new();
    harness.set_observer(record_replies(replies.clone()));

    let client = harness.new_client().expect("client bootstraps");
    let pool = ClientPool::from_client(client, 16).expect("client pool");

    // Pre-crash: complete a batch with the full 2f+1 quorum.
    pool.submit_n(5, 1).expect("pre-crash submit");
    pool.assert_all_completed(Duration::from_secs(20))
        .expect("pre-crash requests complete");
    assert_eq!(pool.ledger().completed_count(), 5);

    // Crash f+1 = 2 replicas → only 2f = 2 survive, below the 2f+1 quorum.
    harness.crash(NodeId::from(2u32)).expect("crash node 2");
    harness.crash(NodeId::from(3u32)).expect("crash node 3");

    // (ii) HALT: post-crash requests must never complete (no reachable quorum).
    let keys: Vec<u64> = (0..3u64)
        .map(|i| pool.submit_ordered(500 + i, 500 + i).expect("submit post-crash"))
        .collect();
    pool.assert_pending(&keys, Duration::from_secs(8))
        .expect("post-crash requests must halt (stay pending) below quorum");

    // (i) pre-crash completions remain stable (never regress).
    assert_eq!(
        pool.ledger().completed_count(),
        5,
        "already-committed requests must stay completed"
    );

    // (iii) NO FORK: every reply any replica sent for a given op agreed with the others.
    assert!(
        replies.total_replies() > 0,
        "the reply observer must have seen pre-crash replies (else the check is vacuous)"
    );
    assert!(
        !replies.has_conflict(),
        "safety violation: replicas reported conflicting results: {:?}",
        replies.conflicts()
    );
}
