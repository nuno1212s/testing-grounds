//! The shared in-process chaos mesh.
//!
//! One `ChaosMesh` is created per scenario (owned by the `ClusterHarness`) and cloned
//! (as `Arc`) into every node's byte-layer `ChaosConfig`. It is the single broker that
//! moves a `WireMessage` from a sender's outbound `ChaosByteStub` to the destination
//! node's incoming stub, mirroring what a real socket + epoll worker does in
//! `atlas-comm-mio` — except the socket hop is collapsed to an in-memory call.
//!
//! Design notes:
//! - **Type-erased delivery.** Replica and client nodes monomorphize the byte layer
//!   over *different* incoming-stub types, so the mesh cannot be generic over them.
//!   Each node registers a `dyn NodeDelivery` (its concrete `NodeEndpoint`) instead.
//! - **The mesh owns connection state** (adjacency + severed set). This is what makes
//!   a `true_crash`'s `sever_all` a single authoritative mutation (F2), and what
//!   answers `has_connection()` — which the reconfiguration protocol really queries.
//! - **Inline delivery.** `dispatch` runs on the process-global threadpool worker
//!   (sends are async, per F1). It calls `handle_message` directly, exactly as the
//!   real epoll worker does; that call only deserializes + pushes to the node's
//!   internal channel, so it never blocks the worker for long.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use atlas_common::node_id::NodeId;
use atlas_communication::message::WireMessage;

use crate::chaos::rules::{EdgeRule, GateDecision, RuleSet};

/// Type-erased per-node delivery endpoint. Registered by each node's
/// `ChaosByteController` at `initialize_controller` time.
pub trait NodeDelivery: Send + Sync {
    /// Deliver a message that arrived at this node, sent by `from`.
    fn deliver(&self, from: NodeId, msg: WireMessage);

    /// Ensure this node has an inbound stub for `from`. Generating that stub also
    /// registers, inside this node's `PeerConnectionManager`, the paired *outbound*
    /// stub to `from` — which is what lets this node subsequently send to `from`.
    fn ensure_inbound(&self, from: NodeId);
}

struct MeshInner {
    /// Live endpoints, keyed by owning node.
    endpoints: BTreeMap<NodeId, Arc<dyn NodeDelivery>>,
    /// Messages addressed to a node whose endpoint has not registered yet.
    /// Drained (in order) the moment that node registers.
    pending: BTreeMap<NodeId, Vec<(NodeId, WireMessage)>>,
    /// Symmetric connection graph (what `has_connection` reports).
    adjacency: BTreeMap<NodeId, BTreeSet<NodeId>>,
    /// Crashed nodes: every message to/from them is dropped and they report
    /// disconnected. Set by `sever_all` (F2).
    severed: BTreeSet<NodeId>,
    /// Fault rules consulted at the gate (F1/F3). Held under the same lock as the
    /// connection state so a phase-precise crash decided by a rule can sever the node
    /// atomically, inline, without re-locking.
    rules: RuleSet,
    /// Per-node stop triggers, so an inline gate crash can also stop the node's thread.
    crash_triggers: BTreeMap<NodeId, Arc<AtomicBool>>,
    /// Optional observation tap invoked for every message a live node emits (before any
    /// drop/crash rule). Used to record what each node actually sent — e.g. its votes,
    /// for equivocation checks — independent of delivery.
    observer: Option<GateObserver>,
}

/// A read-only tap over `(from, to, message)` at the gate.
pub type GateObserver = Arc<dyn Fn(NodeId, NodeId, &WireMessage) + Send + Sync>;

impl MeshInner {
    /// Crash `node` using the already-held lock: sever it and flip its stop trigger.
    fn crash_locked(&mut self, node: NodeId) {
        self.severed.insert(node);
        for (_, set) in self.adjacency.iter_mut() {
            set.remove(&node);
        }
        if let Some(set) = self.adjacency.get_mut(&node) {
            set.clear();
        }
        if let Some(trigger) = self.crash_triggers.get(&node) {
            trigger.store(true, Ordering::SeqCst);
        }
    }
}

pub struct ChaosMesh {
    inner: Mutex<MeshInner>,
}

impl ChaosMesh {
    pub fn new() -> Arc<Self> {
        Arc::new(ChaosMesh {
            inner: Mutex::new(MeshInner {
                endpoints: BTreeMap::new(),
                pending: BTreeMap::new(),
                adjacency: BTreeMap::new(),
                severed: BTreeSet::new(),
                rules: RuleSet::new(),
                crash_triggers: BTreeMap::new(),
                observer: None,
            }),
        })
    }

    /// Install a gate observation tap (see `GateObserver`). At most one; replaces any
    /// prior tap.
    pub fn set_observer(&self, observer: GateObserver) {
        self.inner.lock().unwrap().observer = Some(observer);
    }

    /// Install a fault rule (F1/F3).
    pub fn install_rule(&self, rule: EdgeRule) {
        self.inner.lock().unwrap().rules.install(rule);
    }

    /// Register a node's stop trigger so an inline gate crash can also stop its thread.
    pub fn register_crash_trigger(&self, node: NodeId, trigger: Arc<AtomicBool>) {
        self.inner
            .lock()
            .unwrap()
            .crash_triggers
            .insert(node, trigger);
    }

    /// Register a node's delivery endpoint and flush any messages that arrived before
    /// it was ready. Delivery happens outside the lock (deliver → handle_message may
    /// re-enter the mesh).
    pub fn register_endpoint(&self, node: NodeId, endpoint: Arc<dyn NodeDelivery>) {
        let drained = {
            let mut inner = self.inner.lock().unwrap();
            inner.endpoints.insert(node, endpoint.clone());
            inner.pending.remove(&node).unwrap_or_default()
        };
        for (from, msg) in drained {
            endpoint.deliver(from, msg);
        }
    }

    /// The outbound gate: move `msg` from `from` to `to`, subject to severing/rules.
    /// This runs on a process-global threadpool worker (sends are async, F1), so the
    /// gate — not any decoupled event bus — is the single authority for fault decisions.
    pub fn dispatch(&self, from: NodeId, to: NodeId, msg: WireMessage) {
        let mut inner = self.inner.lock().unwrap();
        if inner.severed.contains(&from) || inner.severed.contains(&to) {
            return;
        }
        // Observe what this (live) node emitted, before any drop/crash rule applies.
        if let Some(obs) = inner.observer.clone() {
            obs(from, to, &msg);
        }
        // Consult the fault rules and act atomically while holding the lock. A
        // phase-precise crash (F1) severs the sender here so this very message — and all
        // its other in-flight copies — are dropped before any peer sees them.
        match inner.rules.evaluate(from, to, &msg) {
            GateDecision::Deliver => {}
            GateDecision::Drop => return,
            GateDecision::CrashThenDrop(target) => {
                inner.crash_locked(target);
                return;
            }
        }
        match inner.endpoints.get(&to).cloned() {
            Some(ep) => {
                tracing::trace!(target: "chaos::gate", "deliver {:?}->{:?} module={:?}", from, to, msg.message_module());
                // Release the mesh lock before delivering: handle_message may re-enter
                // the mesh (e.g. a synchronous reply path).
                drop(inner);
                ep.deliver(from, msg);
            }
            None => {
                // Destination not up yet — buffer and return without blocking a worker.
                inner.pending.entry(to).or_default().push((from, msg));
            }
        }
    }

    /// Establish a (symmetric) connection initiated by `caller` towards `peer`.
    /// Ensures the caller can send to `peer` immediately, and — if `peer` is already
    /// up — that `peer` can send back.
    pub fn connect(&self, caller: NodeId, peer: NodeId) {
        let (caller_ep, peer_ep) = {
            let mut inner = self.inner.lock().unwrap();
            inner.adjacency.entry(caller).or_default().insert(peer);
            inner.adjacency.entry(peer).or_default().insert(caller);
            (
                inner.endpoints.get(&caller).cloned(),
                inner.endpoints.get(&peer).cloned(),
            )
        };
        if let Some(ep) = caller_ep {
            ep.ensure_inbound(peer);
        }
        if let Some(ep) = peer_ep {
            ep.ensure_inbound(caller);
        }
    }

    pub fn disconnect(&self, caller: NodeId, peer: NodeId) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(set) = inner.adjacency.get_mut(&caller) {
            set.remove(&peer);
        }
        if let Some(set) = inner.adjacency.get_mut(&peer) {
            set.remove(&caller);
        }
    }

    pub fn has_connection(&self, node: NodeId, peer: NodeId) -> bool {
        let inner = self.inner.lock().unwrap();
        if inner.severed.contains(&node) || inner.severed.contains(&peer) {
            return false;
        }
        inner
            .adjacency
            .get(&node)
            .is_some_and(|set| set.contains(&peer))
    }

    pub fn connected_nodes(&self, node: NodeId) -> Vec<NodeId> {
        let inner = self.inner.lock().unwrap();
        if inner.severed.contains(&node) {
            return Vec::new();
        }
        inner
            .adjacency
            .get(&node)
            .map(|set| {
                set.iter()
                    .copied()
                    .filter(|n| !inner.severed.contains(n))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// F2: mark `node` crashed. All traffic to/from it drops, it reports disconnected
    /// everywhere, and its stop trigger is flipped. Same effect as an inline gate crash.
    pub fn sever_all(&self, node: NodeId) {
        self.inner.lock().unwrap().crash_locked(node);
    }

    /// Alias for `sever_all` with intent-revealing name for lifecycle callers.
    pub fn crash_node(&self, node: NodeId) {
        self.inner.lock().unwrap().crash_locked(node);
    }

    /// Whether `node` has been severed (crashed).
    pub fn is_severed(&self, node: NodeId) -> bool {
        self.inner.lock().unwrap().severed.contains(&node)
    }

    /// Undo a sever (used by `restart_with_state`/`restart_amnesia`).
    pub fn unsever(&self, node: NodeId) {
        let mut inner = self.inner.lock().unwrap();
        inner.severed.remove(&node);
    }
}
