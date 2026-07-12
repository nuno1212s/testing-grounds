//! Protocol adapters: byte-layer content classifiers that recognize a specific
//! protocol's messages at the chaos gate.
//!
//! M1 ships the febft adapter only. M3 factors the shared shape into a `PhaseObserver` /
//! `CommonPhaseEvent` core with per-adapter crash-points (F5) and adds HotIron/IronChain.

pub mod febft;
