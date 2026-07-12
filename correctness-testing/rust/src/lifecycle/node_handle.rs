//! Per-replica lifecycle management: spawn a real `MonReplica` on its own thread over
//! the chaos transport, stop it gracefully, or crash it.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use atlas_common::node_id::NodeId;

use crate::chaos::ChaosMesh;
use crate::lifecycle::backend::ProtocolBackend;
use crate::lifecycle::config::TimingConfig;
use crate::lifecycle::identity::ClusterIdentity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeLifecycleState {
    Running,
    GracefullyStopped,
    Crashed,
}

pub struct NodeHandle {
    pub node_id: NodeId,
    // The persistent-log directory: reused by restart_with_state, replaced by restart_amnesia.
    db_path: PathBuf,
    stop_trigger: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    mesh: Arc<ChaosMesh>,
    state: NodeLifecycleState,
}

impl NodeHandle {
    /// Spawn a replica bootstrap thread. Because `Replica::bootstrap` blocks on the
    /// reconfiguration protocol reaching a stable quorum (server/mod.rs:459), all N
    /// replicas must be spawned before any of them can finish bootstrapping — hence the
    /// `ready` channel: the caller fires every `spawn` first, then waits for all
    /// readies. `ready` carries `Ok(())` once `bootstrap` returns, or `Err` on failure.
    pub fn spawn<P: ProtocolBackend>(
        node_id: NodeId,
        identity: &ClusterIdentity,
        mesh: Arc<ChaosMesh>,
        db_path: PathBuf,
        timing: TimingConfig,
        node_config: P::NodeConfig,
        ready: Sender<Result<(), String>>,
    ) -> Self {
        let stop_trigger = Arc::new(AtomicBool::new(false));
        // Register so an inline gate crash (F1) can also stop this node's thread.
        mesh.register_crash_trigger(node_id, stop_trigger.clone());

        let reconfig = identity.replica_reconfig_config(node_id);
        let mesh_for_thread = mesh.clone();
        let stop_for_thread = stop_trigger.clone();
        let db_path_str = db_path.to_string_lossy().into_owned();

        let thread = std::thread::Builder::new()
            .name(format!("replica-{}", node_id.0))
            .spawn(move || {
                P::bootstrap_and_run(
                    node_id,
                    reconfig,
                    mesh_for_thread,
                    db_path_str,
                    timing,
                    node_config,
                    ready,
                    stop_for_thread,
                );
            })
            .expect("spawn replica thread");

        NodeHandle {
            node_id,
            db_path,
            stop_trigger,
            thread: Some(thread),
            mesh,
            state: NodeLifecycleState::Running,
        }
    }

    pub fn state(&self) -> NodeLifecycleState {
        self.state
    }

    pub fn db_path(&self) -> &std::path::Path {
        &self.db_path
    }

    /// Cooperative stop: flip the trigger and join. `MonReplica::run` finishes its
    /// current `iterate()` and returns. Not a crash.
    pub fn graceful_stop(&mut self) {
        if self.state != NodeLifecycleState::Running {
            return;
        }
        self.stop_trigger.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        self.state = NodeLifecycleState::GracefullyStopped;
    }

    /// F2: crash — sever the node in the mesh (drop all its traffic + report it
    /// disconnected, which `has_connection` honors) and abandon its thread without a
    /// clean join. We still set the stop trigger so the detached thread eventually
    /// unwinds instead of spinning forever.
    pub fn true_crash(&mut self) {
        if self.state == NodeLifecycleState::Crashed {
            return;
        }
        self.mesh.sever_all(self.node_id);
        self.stop_trigger.store(true, Ordering::Relaxed);
        // Deliberately DO NOT join: a real crash gives no graceful shutdown.
        self.thread.take();
        self.state = NodeLifecycleState::Crashed;
    }
}
