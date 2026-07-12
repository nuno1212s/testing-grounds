//! In-process chaos transport: a genuine implementation of the Atlas byte-network
//! traits (`ByteNetworkController` / `ByteNetworkControllerInit` / `ByteNetworkStub` /
//! `NetworkConnectionController`) that moves messages between co-located replica/client
//! objects over in-memory channels instead of TCP, with a central mesh that can drop,
//! sever, and (M1+) delay/reorder/duplicate messages at exact protocol phases.

pub mod conn_controller;
pub mod controller;
pub mod mesh;
pub mod rules;
pub mod stub;

pub use controller::{ChaosByteController, ChaosConfig};
pub use mesh::{ChaosMesh, GateObserver};
pub use rules::{EdgeRule, Predicate, RuleAction};
pub use stub::ChaosByteStub;
