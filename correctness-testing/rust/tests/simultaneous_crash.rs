//! M5: simultaneous crash of exactly f replicas — the cluster stays live.
//!
//! With n=4/f=1, crashing f=1 replica up front leaves 2f+1=3 survivors, exactly a live
//! quorum (zero slack, per F4 — hence a generous deadline). The oracle confirms every
//! request submitted *after* the crash still completes.
//!
//! febft-only for now (HotStuff/IronChain crash-stop liveness is an open item — FU-2).
//! Run with `cargo nextest run`.

mod common;

use std::time::Duration;

use correctness_testing::oracle::ClientPool;
use correctness_testing::{ClusterHarness, Febft, NodeId};

#[test]
fn simultaneous_crash_of_f_stays_live() {
    common::init_tracing();

    let mut harness = ClusterHarness::<Febft>::new(4).expect("cluster bootstraps");
    let client = harness.new_client().expect("client bootstraps");
    let pool = ClientPool::from_client(client, 16).expect("client pool");

    // Crash exactly f=1 replica (a non-leader), up front.
    harness.crash(NodeId::from(3u32)).expect("crash 1 replica");

    // 2f+1 = 3 survivors remain a live quorum — everything still completes.
    pool.submit_n(10, 1).expect("submit load");
    pool.assert_all_completed(Duration::from_secs(30))
        .expect("with f crashed, the 2f+1 survivors must keep completing requests");

    assert_eq!(pool.ledger().completed_count(), 10);
}
