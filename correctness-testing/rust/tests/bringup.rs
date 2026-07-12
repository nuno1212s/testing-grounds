//! M0/M3 bring-up gate, across all three protocols.
//!
//! Bootstraps 4 real `MonReplica`s + a client in one process over the chaos transport and
//! gets one request agreed and answered — for febft, HotIron, and IronChain. nextest runs
//! each generated test in its own process, which also proves per-scenario singleton
//! isolation (no `InitPoolError`).
//!
//! Run with `cargo nextest run`.

mod common;

use correctness_testing::{ClusterHarness, ProtocolBackend, protocol_test};

fn bringup_one_request<P: ProtocolBackend>() {
    common::init_tracing();

    let mut harness = ClusterHarness::<P>::new(4).expect("4-replica cluster should bootstrap");
    assert_eq!(harness.n(), 4);
    assert_eq!(harness.f(), 1);

    let reply = harness
        .submit_one_ordered(42, 7)
        .expect("one ordered request should be agreed and answered");

    assert_eq!(reply.value, 7, "echo should return the written value");
    assert_eq!(reply.op_count, 1, "exactly one ordered op should be applied");
}

protocol_test!(bringup_one_request => [Febft, HotStuff, ChainedHotStuff]);
