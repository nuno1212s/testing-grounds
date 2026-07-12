//! Protocol backends: the one seam that differs per ordering protocol.
//!
//! Everything else in the harness (transport, PKI, client pool, oracle, fault engine) is
//! protocol-agnostic. A `ProtocolBackend` encapsulates (a) any per-node protocol config
//! (febft: none; HotIron: a threshold-crypto `QuorumInfo` share) and (b) bootstrapping +
//! running the concrete `MonReplica` for that protocol.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use atlas_common::async_runtime;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::SeqNo;
use atlas_decision_log::config::DecLogConfig;
use atlas_log_transfer::config::LogTransferConfig;
use atlas_reconfiguration::config::ReconfigurableNetworkConfig;
use atlas_view_transfer::config::ViewTransferConfig;
use febft_pbft_consensus::bft::config::{PBFTConfig, ProposerConfig};
use febft_state_transfer::config::StateTransferConfig;
use hot_iron_oxide::config::HotIronInitConfig;
use hot_iron_oxide::crypto::QuorumInfo;

use crate::apps::kv_echo::KvEcho;
use crate::chaos::{ChaosConfig, ChaosMesh};
use crate::composition;
use crate::lifecycle::config::TimingConfig;

/// The protocol-specific seam. `ClusterHarness<P>` and `NodeHandle::spawn::<P>` are
/// generic over this.
pub trait ProtocolBackend: Send + Sync + 'static {
    /// Per-node protocol config (febft: `()`, HotIron: `QuorumInfo`).
    type NodeConfig: Send + 'static;

    /// A short protocol name, for logging/labels.
    fn label() -> &'static str;

    /// One config per replica, index `i` → node `i`.
    fn generate_node_configs(n: usize, f: usize) -> Vec<Self::NodeConfig>;

    /// Bootstrap a replica on the calling thread, signal `ready` once `bootstrap`
    /// returns, then run it until `stop` is set.
    fn bootstrap_and_run(
        node_id: NodeId,
        reconfig: ReconfigurableNetworkConfig,
        mesh: Arc<ChaosMesh>,
        db_path: String,
        timing: TimingConfig,
        node_config: Self::NodeConfig,
        ready: Sender<Result<(), String>>,
        stop: Arc<AtomicBool>,
    );
}

// ── Shared transfer/log configs (identical struct types across protocols) ────────────
fn dec_log_config() -> DecLogConfig {
    DecLogConfig {
        default_ongoing_capacity: 1024,
    }
}
fn lt_config(t: &TimingConfig) -> LogTransferConfig {
    LogTransferConfig {
        timeout_duration: t.log_transfer_timeout,
    }
}
fn vt_config(t: &TimingConfig) -> ViewTransferConfig {
    ViewTransferConfig {
        timeout_duration: t.view_transfer_timeout,
    }
}
fn st_config(t: &TimingConfig) -> StateTransferConfig {
    StateTransferConfig {
        timeout_duration: t.state_transfer_timeout,
    }
}

/// Drive a freshly-bootstrapped replica: signal ready, then loop `run` until stopped.
/// `$comp` is the protocol's composition module.
macro_rules! run_replica {
    ($comp:path, $mon:expr, $ready:expr, $stop:expr, $node_id:expr) => {{
        use $comp as comp;
        let boot = async_runtime::block_on(comp::SMRReplica::bootstrap($mon));
        let mut replica: comp::SMRReplica = match boot {
            Ok(r) => {
                let _ = $ready.send(Ok(()));
                r
            }
            Err(e) => {
                let _ = $ready.send(Err(format!("bootstrap failed: {e}")));
                return;
            }
        };
        loop {
            if $stop.load(Ordering::Relaxed) {
                break;
            }
            match replica.run(Some($stop.clone())) {
                Ok(()) => break,
                Err(e) => {
                    if $stop.load(Ordering::Relaxed) {
                        break;
                    }
                    tracing::error!("replica {:?} run error: {}", $node_id, e);
                }
            }
        }
    }};
}

/// A backend whose per-node config can be (re)produced for a crashed node, enabling
/// `restart_amnesia` / `restart_with_state`. febft qualifies trivially (`NodeConfig = ()`).
/// The HotStuff family does NOT: its per-node `QuorumInfo` threshold share is randomly
/// generated, not `Clone`, and consumed by bootstrap — so a restarted node cannot recover
/// the same share (tracked as FU-6). Restart methods are only available for `P:
/// RestartableBackend`.
pub trait RestartableBackend: ProtocolBackend {
    /// Produce the config for (re)starting `node` in an `n`-replica, f-tolerant cluster.
    fn node_config_for(n: usize, f: usize, node: NodeId) -> Self::NodeConfig;
}

// ── febft ────────────────────────────────────────────────────────────────────────────
pub struct Febft;

impl RestartableBackend for Febft {
    fn node_config_for(_n: usize, _f: usize, _node: NodeId) {}
}

impl ProtocolBackend for Febft {
    type NodeConfig = ();

    fn label() -> &'static str {
        "febft"
    }

    fn generate_node_configs(n: usize, _f: usize) -> Vec<()> {
        vec![(); n]
    }

    fn bootstrap_and_run(
        node_id: NodeId,
        reconfig: ReconfigurableNetworkConfig,
        mesh: Arc<ChaosMesh>,
        db_path: String,
        timing: TimingConfig,
        _node_config: (),
        ready: Sender<Result<(), String>>,
        stop: Arc<AtomicBool>,
    ) {
        let op_config = PBFTConfig::new(
            timing.consensus_timeout,
            128,
            ProposerConfig::new(
                timing.target_batch_size,
                timing.max_batch_size,
                timing.batch_timeout_micros,
                2,
            ),
        );
        let replica_config = composition::febft::ReplicaConf {
            node: ChaosConfig { mesh },
            next_consensus_seq: SeqNo::ZERO,
            op_config,
            dl_config: dec_log_config(),
            lt_config: lt_config(&timing),
            db_path,
            pl_config: (),
            reconfig_node: reconfig,
            vt_config: vt_config(&timing),
            p: Default::default(),
            preprocessor_threads: 2,
        };
        let mon = composition::febft::MonConfig {
            service: KvEcho::new(),
            replica_config,
            st_config: st_config(&timing),
        };
        run_replica!(composition::febft, mon, ready, stop, node_id);
    }
}

// ── HotIron ────────────────────────────────────────────────────────────────────────
pub struct HotStuff;

impl ProtocolBackend for HotStuff {
    type NodeConfig = QuorumInfo;

    fn label() -> &'static str {
        "hotstuff"
    }

    fn generate_node_configs(_n: usize, f: usize) -> Vec<QuorumInfo> {
        // Returns exactly n = 3f+1 threshold shares, indexed by node.
        QuorumInfo::initialize(f)
    }

    fn bootstrap_and_run(
        node_id: NodeId,
        reconfig: ReconfigurableNetworkConfig,
        mesh: Arc<ChaosMesh>,
        db_path: String,
        timing: TimingConfig,
        node_config: QuorumInfo,
        ready: Sender<Result<(), String>>,
        stop: Arc<AtomicBool>,
    ) {
        let op_config = HotIronInitConfig {
            quorum_info: node_config,
        };
        let replica_config = composition::hotstuff::ReplicaConf {
            node: ChaosConfig { mesh },
            next_consensus_seq: SeqNo::ZERO,
            op_config,
            dl_config: dec_log_config(),
            lt_config: lt_config(&timing),
            db_path,
            pl_config: (),
            reconfig_node: reconfig,
            vt_config: vt_config(&timing),
            p: Default::default(),
            preprocessor_threads: 2,
        };
        let mon = composition::hotstuff::MonConfig {
            service: KvEcho::new(),
            replica_config,
            st_config: st_config(&timing),
        };
        run_replica!(composition::hotstuff, mon, ready, stop, node_id);
    }
}

// ── IronChain (chained HotStuff) ─────────────────────────────────────────────────────
pub struct ChainedHotStuff;

impl ProtocolBackend for ChainedHotStuff {
    type NodeConfig = QuorumInfo;

    fn label() -> &'static str {
        "chained-hotstuff"
    }

    fn generate_node_configs(_n: usize, f: usize) -> Vec<QuorumInfo> {
        QuorumInfo::initialize(f)
    }

    fn bootstrap_and_run(
        node_id: NodeId,
        reconfig: ReconfigurableNetworkConfig,
        mesh: Arc<ChaosMesh>,
        db_path: String,
        timing: TimingConfig,
        node_config: QuorumInfo,
        ready: Sender<Result<(), String>>,
        stop: Arc<AtomicBool>,
    ) {
        let op_config = HotIronInitConfig {
            quorum_info: node_config,
        };
        let replica_config = composition::chained::ReplicaConf {
            node: ChaosConfig { mesh },
            next_consensus_seq: SeqNo::ZERO,
            op_config,
            dl_config: dec_log_config(),
            lt_config: lt_config(&timing),
            db_path,
            pl_config: (),
            reconfig_node: reconfig,
            vt_config: vt_config(&timing),
            p: Default::default(),
            preprocessor_threads: 2,
        };
        let mon = composition::chained::MonConfig {
            service: KvEcho::new(),
            replica_config,
            st_config: st_config(&timing),
        };
        run_replica!(composition::chained, mon, ready, stop, node_id);
    }
}
