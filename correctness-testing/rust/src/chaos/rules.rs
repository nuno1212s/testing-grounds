//! Fault rules evaluated inline at the byte gate.
//!
//! A `RuleSet` is a list of `EdgeRule`s consulted on every outbound message. Coarse
//! keying `(from, to, module)` is free (no deserialization). A protocol-specific content
//! `Predicate: Fn(&WireMessage) -> bool` (F3) can additionally inspect the deserialized
//! payload — that is what makes an edge rule phase-precise while keeping the chaos layer
//! protocol-agnostic (it only stores boxed predicates).
//!
//! Phase-triggered crashes are decided INLINE here (F1): the same match that recognizes
//! e.g. "the leader's COMMIT for seq N" both drops that message and severs the sender, so
//! the triggering message never leaks to peers.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use atlas_common::node_id::NodeId;
use atlas_communication::lookup_table::MessageModule;
use atlas_communication::message::WireMessage;

/// A boxed, protocol-supplied content predicate over a fully-formed wire message.
pub type Predicate = Arc<dyn Fn(&WireMessage) -> bool + Send + Sync>;

/// What the gate does when a rule matches.
#[derive(Clone)]
pub enum RuleAction {
    /// Swallow the matched message (drop / omission).
    Drop,
    /// Crash `target` inline (F1): sever it so this very message — and every other
    /// message it has in flight — is dropped, and stop its thread. Then drop the
    /// matched message.
    CrashNode(NodeId),
}

/// The fate the gate should apply to a message after consulting the rules.
#[derive(Clone)]
pub(crate) enum GateDecision {
    Deliver,
    Drop,
    /// Crash this node (performed inline by the mesh), then drop the message.
    CrashThenDrop(NodeId),
}

/// One fault rule. Coarse fields are ANDed; `None` means "any". `predicate`, if present,
/// must also return true.
pub struct EdgeRule {
    pub from: Option<NodeId>,
    pub to: Option<NodeId>,
    pub module: Option<MessageModule>,
    pub predicate: Option<Predicate>,
    pub action: RuleAction,
    /// Fire at most once, then the rule is removed.
    pub once: bool,
    /// Set to true by the gate the first time this rule matches — an observation hook
    /// the scenario can assert on ("the phase-precise fault actually fired").
    pub fired: Arc<AtomicBool>,
}

impl EdgeRule {
    /// A general drop rule. Any `None` field means "any".
    pub fn drop_matching(
        from: Option<NodeId>,
        to: Option<NodeId>,
        module: Option<MessageModule>,
        predicate: Option<Predicate>,
    ) -> Self {
        EdgeRule {
            from,
            to,
            module,
            predicate,
            action: RuleAction::Drop,
            once: false,
            fired: Arc::new(AtomicBool::new(false)),
        }
    }

    /// A rule that drops every message on edge `(from -> to)` for the given module.
    pub fn drop_edge(from: NodeId, to: NodeId, module: Option<MessageModule>) -> Self {
        Self::drop_matching(Some(from), Some(to), module, None)
    }

    /// **Send-omission**: drop everything `node` sends. Evaluated on `node`'s outbound
    /// edges, so `node`'s own send-side logic still believes it sent (the message just
    /// never reaches anyone). `filter` restricts to matching messages (`None` = all).
    pub fn send_omission(node: NodeId, filter: Option<Predicate>) -> Self {
        Self::drop_matching(Some(node), None, None, filter)
    }

    /// **Receive-omission**: drop everything addressed to `node`. Senders' send-side
    /// logic is unaffected; `node` simply never receives. `filter` restricts to matching
    /// messages (`None` = all).
    pub fn receive_omission(node: NodeId, filter: Option<Predicate>) -> Self {
        Self::drop_matching(None, Some(node), None, filter)
    }

    /// A phase-precise crash: when `node` emits a message matching `predicate`, crash it.
    pub fn crash_on(node: NodeId, module: Option<MessageModule>, predicate: Predicate) -> Self {
        EdgeRule {
            from: Some(node),
            to: None,
            module,
            predicate: Some(predicate),
            action: RuleAction::CrashNode(node),
            once: true,
            fired: Arc::new(AtomicBool::new(false)),
        }
    }

    /// A handle to observe whether this rule has fired.
    pub fn fired_flag(&self) -> Arc<AtomicBool> {
        self.fired.clone()
    }

    fn matches(&self, from: NodeId, to: NodeId, msg: &WireMessage) -> bool {
        if self.from.is_some_and(|f| f != from) {
            return false;
        }
        if self.to.is_some_and(|t| t != to) {
            return false;
        }
        if let Some(m) = &self.module {
            if m != msg.message_module() {
                return false;
            }
        }
        if let Some(pred) = &self.predicate {
            if !pred(msg) {
                return false;
            }
        }
        true
    }
}

/// The active set of edge rules.
#[derive(Default)]
pub struct RuleSet {
    rules: Vec<EdgeRule>,
}

impl RuleSet {
    pub fn new() -> Self {
        RuleSet::default()
    }

    pub fn install(&mut self, rule: EdgeRule) {
        self.rules.push(rule);
    }

    /// Consult the rules for an outbound `(from -> to)` message. Returns the fate to
    /// apply. Marks matched rules as fired and removes spent `once` rules.
    pub(crate) fn evaluate(
        &mut self,
        from: NodeId,
        to: NodeId,
        msg: &WireMessage,
    ) -> GateDecision {
        let mut decision = GateDecision::Deliver;
        let mut remove_idx: Option<usize> = None;

        for (idx, rule) in self.rules.iter().enumerate() {
            if !rule.matches(from, to, msg) {
                continue;
            }
            rule.fired.store(true, std::sync::atomic::Ordering::SeqCst);
            decision = match rule.action {
                RuleAction::Drop => GateDecision::Drop,
                RuleAction::CrashNode(target) => GateDecision::CrashThenDrop(target),
            };
            if rule.once {
                remove_idx = Some(idx);
            }
            // First matching rule wins.
            break;
        }

        if let Some(idx) = remove_idx {
            self.rules.remove(idx);
        }

        decision
    }
}
