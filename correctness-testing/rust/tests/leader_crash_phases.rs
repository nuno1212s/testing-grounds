//! M1: phase-precise leader crash via the inline byte gate.
//!
//! This is the F1/F3/F2 showcase. It validates that:
//!   * a protocol adapter can recognize a febft message by CONTENT at the byte layer
//!     (F3) — here, "an outbound COMMIT" — by deserializing the wire payload;
//!   * the crash is armed and fires INLINE at the gate, atomically, the moment that
//!     COMMIT is about to be dispatched (F1), so it never leaks to peers;
//!   * the crash severs the node in the mesh (F2).
//!
//! It deliberately does NOT assert post-crash liveness: recovering a crashed leader needs
//! a view change, which is a separate open item. It asserts the fault MECHANISM.
//!
//! Run with `cargo nextest run`.

mod common;

use std::sync::atomic::Ordering;
use std::thread::sleep;
use std::time::{Duration, Instant};

use correctness_testing::adapters::febft;
use correctness_testing::{ClusterHarness, NodeId};

/// Arm "crash the leader as it is about to broadcast a COMMIT", then confirm the inline
/// gate recognizes a real febft COMMIT and crashes the leader.
#[test]
fn leader_crashes_inline_on_commit() {
    common::init_tracing();

    let mut harness = ClusterHarness::<correctness_testing::Febft>::new(4).expect("cluster bootstraps");
    let mut client = harness.new_client().expect("client bootstraps");

    // Baseline: the cluster orders and answers a request before any fault.
    let reply = harness.submit_ordered(&mut client, 1, 10).expect("baseline request");
    assert_eq!(reply.value, 10);

    let leader = harness.initial_leader();
    assert!(!harness.is_crashed(leader), "leader alive pre-fault");

    // Arm the phase-precise crash. febft keeps consensus advancing with periodic batches,
    // so the leader will emit a COMMIT shortly; `seq = None` matches the next one.
    let rule = febft::crash_leader_before_commit_broadcast(leader, None);
    let fired = rule.fired_flag();
    harness.install_rule(rule);

    // Wait for the gate to recognize a COMMIT and fire the inline crash.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !fired.load(Ordering::SeqCst) && Instant::now() < deadline {
        sleep(Duration::from_millis(20));
    }

    assert!(
        fired.load(Ordering::SeqCst),
        "the byte-gate COMMIT predicate should have matched and fired the inline crash"
    );
    assert!(
        harness.is_crashed(leader),
        "the leader should be severed (crashed) by the inline gate action (F1/F2)"
    );

    // And the survivors must see it as disconnected (F2: connected-set updated).
    let survivor = NodeId::from(1u32);
    assert!(
        !harness.mesh().has_connection(survivor, leader),
        "survivors must report the crashed leader as disconnected"
    );
}
