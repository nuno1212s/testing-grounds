//! A record of the replies each replica sent to clients, observed at the byte gate.
//!
//! Every replica→client reply is an `Application`-module `OrderableMessage::Ordered/
//! UnorderedReply(ReplyMessage)` — app-level and therefore protocol-agnostic. Recording
//! them per `(session, operation)` gives a direct **execution-level fork check**: if two
//! replicas ever reported *different* results for the same client operation, that is a
//! safety violation. Used by the halt-not-fork scenario (and any scenario wanting "did the
//! survivors agree on what they executed").

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use atlas_common::node_id::NodeId;
use atlas_common::ordering::Orderable;
use atlas_communication::lookup_table::MessageModule;
use atlas_communication::message::WireMessage;
use atlas_communication::serialization::deserialize_message;
use atlas_smr_core::message::OrderableMessage;
use atlas_smr_core::serialize::SMRSysMsg;

use crate::chaos::GateObserver;
use crate::composition::AppData;

/// Key: (session, operation id). Value: per-replica reply payload (debug string).
type ReplyKey = (i64, i64);

#[derive(Clone, Default)]
pub struct ReplyLog {
    inner: Arc<Mutex<BTreeMap<ReplyKey, BTreeMap<NodeId, String>>>>,
}

impl ReplyLog {
    pub fn new() -> Self {
        ReplyLog {
            inner: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn record(&self, from: NodeId, session: i64, op: i64, payload: String) {
        self.inner
            .lock()
            .unwrap()
            .entry((session, op))
            .or_default()
            .insert(from, payload);
    }

    /// Total distinct (replica, session, op) replies observed.
    pub fn total_replies(&self) -> usize {
        self.inner.lock().unwrap().values().map(|m| m.len()).sum()
    }

    /// Every `(session, op)` for which two replicas reported DIFFERENT reply payloads —
    /// an execution-level fork.
    pub fn conflicts(&self) -> Vec<(ReplyKey, Vec<String>)> {
        self.inner
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(key, per_replica)| {
                let distinct: BTreeSet<&String> = per_replica.values().collect();
                if distinct.len() > 1 {
                    Some((*key, distinct.into_iter().cloned().collect()))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn has_conflict(&self) -> bool {
        self.inner.lock().unwrap().values().any(|per_replica| {
            per_replica.values().collect::<BTreeSet<_>>().len() > 1
        })
    }

    /// Every replica that has sent at least one reply we observed.
    pub fn repliers(&self) -> BTreeSet<NodeId> {
        self.inner
            .lock()
            .unwrap()
            .values()
            .flat_map(|per_replica| per_replica.keys().copied())
            .collect()
    }
}

/// A gate observer that records every replica→client reply into `log`. Protocol-agnostic
/// (replies are app-level `OrderableMessage`s).
pub fn record_replies(log: ReplyLog) -> GateObserver {
    Arc::new(move |from: NodeId, _to: NodeId, msg: &WireMessage| {
        if *msg.message_module() != MessageModule::Application {
            return;
        }
        let Ok(m) = deserialize_message::<&[u8], SMRSysMsg<AppData>>(msg.payload()) else {
            return;
        };
        let reply = match m {
            OrderableMessage::OrderedReply(r) | OrderableMessage::UnorderedReply(r) => r,
            _ => return, // requests (client→replica) are not replies
        };
        let session = i64::from(u32::from(reply.session_id()));
        let op = i64::from(u32::from(reply.sequence_number()));
        log.record(from, session, op, format!("{:?}", reply.payload()));
    })
}
