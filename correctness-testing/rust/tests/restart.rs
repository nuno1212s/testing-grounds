//! FU-6: node restart (crash-recovery lifecycle).
//!
//! Crash a replica, then bring it back with `restart_amnesia` (fresh empty db) and confirm
//! it rejoins the quorum and the cluster remains usable. This is the foundation for the
//! amnesia double-vote scenario and the M4 crash-recovery scenarios. febft-only
//! (`RestartableBackend`; the HotStuff family's threshold shares aren't reproducible).
//!
//! Run with `cargo nextest run`.

mod common;

use correctness_testing::{ClusterHarness, Febft, NodeId};

#[test]
fn restart_amnesia_rejoins_and_cluster_stays_usable() {
    common::init_tracing();

    let mut harness = ClusterHarness::<Febft>::new(4).expect("cluster bootstraps");
    let mut client = harness.new_client().expect("client bootstraps");

    assert_eq!(harness.submit_ordered(&mut client, 1, 10).unwrap().value, 10);

    let node = NodeId::from(3u32);
    harness.crash(node).expect("crash node 3");
    assert!(harness.is_crashed(node));

    // 2f+1 survivors keep the cluster live while node 3 is down.
    assert_eq!(harness.submit_ordered(&mut client, 2, 20).unwrap().value, 20);

    // Bring node 3 back with a fresh empty db; it must rejoin the quorum.
    harness
        .restart_amnesia(node)
        .expect("node should rejoin after an amnesia restart");
    assert!(!harness.is_crashed(node), "restarted node is no longer severed");

    // The cluster remains usable after the rejoin.
    assert_eq!(harness.submit_ordered(&mut client, 3, 30).unwrap().value, 30);
}
