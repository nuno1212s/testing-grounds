//! `correctness-testing`: an in-process, protocol-agnostic fault-injection harness for
//! Atlas ordering protocols.
//!
//! One Rust process spawns N real `MonReplica` objects (+ clients) as threads, wired
//! together by a custom in-memory "chaos" transport that can crash nodes, partition the
//! network, and (M1+) induce faults at exact protocol phases — then asserts that
//! survivors stay safe (no forking) and, when ≤f have failed, stay live.
//!
//! Tests MUST run under `cargo nextest run`: each scenario needs its own OS process
//! because `atlas-common` holds unresettable process-global `OnceLock` singletons.
//!
//! M0 (this milestone) proves the hard part — a full in-process `MonReplica` cluster
//! bootstrapping over the chaos transport and agreeing on one client request, no faults.

pub mod adapters;
pub mod apps;
pub mod chaos;
pub mod composition;
pub mod lifecycle;
pub mod oracle;

pub use lifecycle::{ChainedHotStuff, ClusterHarness, Febft, HotStuff, ProtocolBackend, RestartableBackend};

// Re-exported so scenario tests can name nodes without a direct atlas-common dep.
pub use atlas_common::node_id::NodeId;

/// Expand a generic scenario `fn <P: ProtocolBackend>()` into one `#[test]` per protocol.
///
/// ```ignore
/// fn my_scenario<P: ProtocolBackend>() { let h = ClusterHarness::<P>::new(4)?; ... }
/// protocol_test!(my_scenario => [Febft, HotStuff, ChainedHotStuff]);
/// ```
/// generates `mod my_scenario { #[test] fn Febft() {..} #[test] fn HotStuff() {..} .. }`.
#[macro_export]
macro_rules! protocol_test {
    ($scenario:ident => [$($proto:ident),+ $(,)?]) => {
        mod $scenario {
            $(
                #[test]
                #[allow(non_snake_case)]
                fn $proto() {
                    super::$scenario::<$crate::$proto>();
                }
            )+
        }
    };
}
