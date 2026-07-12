//! `ClusterHarness`: the single object a scenario talks to.
//!
//! Owns the one process-global `InitGuard` (F1/singletons), the shared chaos mesh, the
//! synthesized PKI, and the replica `NodeHandle`s. Brings up all N replicas concurrently
//! (mandatory: `bootstrap` blocks until reconfiguration reaches a stable quorum, so a
//! serial bring-up deadlocks) and can run client requests over the chaos transport.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use atlas_client::client::ordered_client::Ordered;
use atlas_client::client::unordered_client::UnorderedClientMode;
use atlas_client::client::{ClientConfig, bootstrap_client};
use atlas_common::async_runtime;
use atlas_common::node_id::NodeId;
use atlas_common::{InitConfig, InitGuard};

use std::marker::PhantomData;

use crate::apps::kv_echo::{EchoReply, EchoRequest};
use crate::chaos::{ChaosConfig, ChaosMesh};
use crate::composition::{AppData, BFT, ClientNode, ReconfProtocol, SMRClient};
use crate::lifecycle::backend::{ProtocolBackend, RestartableBackend};
use crate::lifecycle::config::TimingConfig;
use crate::lifecycle::identity::ClusterIdentity;
use crate::lifecycle::node_handle::NodeHandle;

/// Client ids start well above any replica id to avoid collisions.
const CLIENT_ID_BASE: u32 = 1000;

/// Process-unique counter so concurrent test processes (and multiple harnesses, if any)
/// never share a persistent-log directory.
static DB_COUNTER: AtomicU32 = AtomicU32::new(0);

pub struct ClusterHarness<P: ProtocolBackend> {
    // Dropped LAST (after replicas are stopped): tearing this down kills the shared
    // async runtime + threadpool. `Option` because a second harness in one process
    // would get `Ok(None)` from `init`.
    init_guard: Option<InitGuard>,
    mesh: Arc<ChaosMesh>,
    identity: ClusterIdentity,
    replicas: BTreeMap<NodeId, NodeHandle>,
    n: usize,
    f: usize,
    timing: TimingConfig,
    next_client_id: u32,
    db_root: PathBuf,
    /// Per-node count of amnesia restarts, so each gets a fresh empty db directory.
    restart_counters: BTreeMap<NodeId, u32>,
    _backend: PhantomData<fn() -> P>,
}

impl<P: ProtocolBackend> ClusterHarness<P> {
    /// Bring up an `n`-replica cluster (no faults). `f = (n-1)/3`.
    pub fn new(n: usize) -> Result<Self> {
        Self::with_timing(n, TimingConfig::default())
    }

    pub fn with_timing(n: usize, timing: TimingConfig) -> Result<Self> {
        assert!(n >= 1, "need at least one replica");
        let f = (n - 1) / 3;

        // Size the shared pools for the whole cluster + a client, with headroom (F1) but
        // without over-allocating — many scenarios run as parallel processes, so keep the
        // per-process footprint modest (nextest also caps parallelism, see nextest.toml).
        let init_guard = unsafe {
            atlas_common::init(InitConfig {
                async_threads: 4,
                threadpool_threads: 8,
            })
        }
        .map_err(|e| anyhow!("atlas_common::init failed: {e}"))?;

        let mesh = ChaosMesh::new();
        let identity = ClusterIdentity::new(n);

        let db_root = std::env::temp_dir().join(format!(
            "atlas-correctness-{}-{}",
            std::process::id(),
            DB_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));

        // Per-node protocol config (febft: units; HotIron: threshold shares).
        let node_configs = P::generate_node_configs(n, f);

        let mut harness = ClusterHarness {
            init_guard,
            mesh,
            identity,
            replicas: BTreeMap::new(),
            n,
            f,
            timing,
            next_client_id: CLIENT_ID_BASE,
            db_root,
            restart_counters: BTreeMap::new(),
            _backend: PhantomData,
        };

        harness.bring_up_all(node_configs)?;
        Ok(harness)
    }

    /// Spawn all replica bootstrap threads, THEN wait for every one to report ready.
    fn bring_up_all(&mut self, node_configs: Vec<P::NodeConfig>) -> Result<()> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

        let ids: Vec<NodeId> = self.identity.replica_ids().to_vec();
        assert_eq!(
            ids.len(),
            node_configs.len(),
            "backend must produce one node config per replica"
        );

        for (node_id, node_config) in ids.iter().copied().zip(node_configs) {
            let db_path = self.db_root.join(format!("node-{}", node_id.0));
            std::fs::create_dir_all(&db_path)
                .map_err(|e| anyhow!("create db dir for {:?}: {e}", node_id))?;

            let handle = NodeHandle::spawn::<P>(
                node_id,
                &self.identity,
                self.mesh.clone(),
                db_path,
                self.timing.clone(),
                node_config,
                ready_tx.clone(),
            );
            self.replicas.insert(node_id, handle);
        }
        drop(ready_tx);

        // Bootstrap drags in the full reconfiguration + transfer machinery; give it room.
        let deadline_per_node = Duration::from_secs(60);
        let mut ready_count = 0;
        for _ in 0..self.n {
            match ready_rx.recv_timeout(deadline_per_node) {
                Ok(Ok(())) => {
                    ready_count += 1;
                    tracing::info!("chaos: replica bootstrapped ({ready_count}/{})", self.n);
                }
                Ok(Err(e)) => return Err(anyhow!("a replica failed to bootstrap: {e}")),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(anyhow!(
                        "timed out waiting for replicas to bootstrap (quorum did not stabilize)"
                    ));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(anyhow!(
                        "a replica thread died before reporting ready (bootstrap panicked?)"
                    ));
                }
            }
        }

        Ok(())
    }

    pub fn n(&self) -> usize {
        self.n
    }

    pub fn f(&self) -> usize {
        self.f
    }

    /// Bootstrap a fresh client wired to the chaos transport. Blocks until the client
    /// joins the (already stable) network.
    pub fn new_client(&mut self) -> Result<SMRClient> {
        let client_id = NodeId::from(self.next_client_id);
        self.next_client_id += 1;

        let reconfig = self.identity.client_reconfig_config(client_id);
        let client_cfg: ClientConfig<ReconfProtocol, AppData, ClientNode> = ClientConfig {
            unordered_rq_mode: UnorderedClientMode::BFT,
            node: ChaosConfig {
                mesh: self.mesh.clone(),
            },
            reconfiguration: reconfig,
        };

        let client = async_runtime::block_on(bootstrap_client::<
            ReconfProtocol,
            AppData,
            ClientNode,
            BFT,
        >(client_id, client_cfg))
        .map_err(|e| anyhow!("client bootstrap failed: {e}"))?;

        Ok(client)
    }

    /// Convenience for M0: bootstrap a client, submit one ordered request, and return
    /// the reply once a quorum of matching replies is received.
    pub fn submit_one_ordered(&mut self, key: u64, value: u64) -> Result<EchoReply> {
        let mut client = self.new_client()?;
        self.submit_ordered(&mut client, key, value)
    }

    /// Submit one ordered request on an existing client and block until a quorum of
    /// matching replies (2f+1) arrives. Reusing a client across requests is how
    /// liveness scenarios drive load before and after a fault.
    pub fn submit_ordered(
        &self,
        client: &mut SMRClient,
        key: u64,
        value: u64,
    ) -> Result<EchoReply> {
        async_runtime::block_on(client.update::<Ordered>(EchoRequest::new(key, value)))
            .map_err(|e| anyhow!("ordered request failed: {e}"))
    }

    /// Crash a replica (F2): sever it in the mesh (all its traffic drops, it reports
    /// disconnected) and abandon its thread without a clean join.
    pub fn crash(&mut self, node_id: NodeId) -> Result<()> {
        let handle = self
            .replicas
            .get_mut(&node_id)
            .ok_or_else(|| anyhow!("no such replica {:?}", node_id))?;
        handle.true_crash();
        Ok(())
    }

    /// The primary of the initial view. febft's leader is view-scoped
    /// (`ViewInfo::leader() = quorum_members[view_seq % n]`), so before any view change
    /// the leader is `quorum_members[0]` = node 0. Crashing this node forces a
    /// timeout-driven view change.
    pub fn initial_leader(&self) -> NodeId {
        NodeId::from(0u32)
    }

    pub fn mesh(&self) -> &Arc<ChaosMesh> {
        &self.mesh
    }

    /// Install a fault rule (F1/F3), e.g. a phase-precise inline crash built from an
    /// adapter content predicate.
    pub fn install_rule(&self, rule: crate::chaos::EdgeRule) {
        self.mesh.install_rule(rule);
    }

    /// Whether a node has been crashed/severed (by `crash`, or inline at the gate).
    pub fn is_crashed(&self, node_id: NodeId) -> bool {
        self.mesh.is_severed(node_id)
    }

    /// Install a gate observation tap (e.g. an adapter vote recorder for equivocation
    /// checks). Protocol-agnostic; the tap itself is protocol-specific.
    pub fn set_observer(&self, observer: crate::chaos::GateObserver) {
        self.mesh.set_observer(observer);
    }

    pub fn replica(&mut self, node_id: NodeId) -> Option<&mut NodeHandle> {
        self.replicas.get_mut(&node_id)
    }

    /// Stop all replicas gracefully. Called by `Drop`, but exposed for explicit teardown.
    pub fn shutdown(&mut self) {
        for (_, handle) in self.replicas.iter_mut() {
            handle.graceful_stop();
        }
    }
}

impl<P: RestartableBackend> ClusterHarness<P> {
    /// Restart a crashed node with a FRESH EMPTY db (amnesia): the recovered node has no
    /// persisted decision log. Exercises Atlas's peer-driven recovery and the WAL-replay
    /// gap (`Atlas-SMR-Replica/src/server/mod.rs:350-353`, where the decision-log read at
    /// bootstrap is discarded).
    pub fn restart_amnesia(&mut self, node: NodeId) -> Result<()> {
        let counter = self.restart_counters.entry(node).or_insert(0);
        *counter += 1;
        let db_path = self
            .db_root
            .join(format!("node-{}-amnesia{}", node.0, counter));
        self.restart_inner(node, db_path)
    }

    /// Restart a crashed node reusing its ORIGINAL db (state preserved). NOTE: reuses the
    /// same persistent-log directory; the crashed instance's threads are abandoned by
    /// `true_crash`, so the db lock must have been released first (best-effort delay).
    pub fn restart_with_state(&mut self, node: NodeId) -> Result<()> {
        let db_path = self
            .replicas
            .get(&node)
            .map(|h| h.db_path().to_path_buf())
            .unwrap_or_else(|| self.db_root.join(format!("node-{}", node.0)));
        self.restart_inner(node, db_path)
    }

    fn restart_inner(&mut self, node: NodeId, db_path: PathBuf) -> Result<()> {
        // Ensure the old instance is severed + signalled to stop, then removed. Its threads
        // are abandoned (a real crash gives no clean shutdown); give them a moment to
        // unwind and release the persistent-log lock before the new instance opens a db.
        if let Some(mut old) = self.replicas.remove(&node) {
            old.true_crash();
        }
        std::thread::sleep(Duration::from_millis(300));

        // Allow the node back onto the network so reconfiguration can re-admit it.
        self.mesh.unsever(node);

        std::fs::create_dir_all(&db_path)
            .map_err(|e| anyhow!("create db dir for restart of {:?}: {e}", node))?;

        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let handle = NodeHandle::spawn::<P>(
            node,
            &self.identity,
            self.mesh.clone(),
            db_path,
            self.timing.clone(),
            P::node_config_for(self.n, self.f, node),
            ready_tx,
        );

        match ready_rx.recv_timeout(Duration::from_secs(60)) {
            Ok(Ok(())) => {
                self.replicas.insert(node, handle);
                Ok(())
            }
            Ok(Err(e)) => Err(anyhow!("restart of {:?} failed to bootstrap: {e}", node)),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(anyhow!(
                "restart of {:?} timed out — the node did not rejoin the quorum",
                node
            )),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(anyhow!(
                "restart of {:?} panicked during bootstrap",
                node
            )),
        }
    }
}

impl<P: ProtocolBackend> Drop for ClusterHarness<P> {
    fn drop(&mut self) {
        // Stop replicas BEFORE the InitGuard drops (which tears down the runtime).
        self.shutdown();
        // Drop the guard explicitly for ordering clarity.
        let _ = self.init_guard.take();
        // Best-effort cleanup of the persistent-log scratch dirs.
        let _ = std::fs::remove_dir_all(&self.db_root);
    }
}
