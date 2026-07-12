//! febft byte-layer content predicates.
//!
//! These are the protocol-specific classifiers the chaos gate uses to recognize a febft
//! message by *content* (F3). Every peer protocol message crosses
//! `ChaosByteStub::dispatch_blocking(WireMessage)`; for the `Protocol` module the payload
//! is a serde-serialized `SystemMessage`, so we deserialize it with the concrete
//! `ProtocolDataType` and match on the febft `PBFTMessage` inside. The chaos layer stays
//! agnostic — it only stores the boxed `Fn(&WireMessage) -> bool`.

use std::sync::Arc;

use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_communication::lookup_table::MessageModule;
use atlas_communication::message::WireMessage;
use atlas_communication::serialization::deserialize_message;
use atlas_smr_core::SMRReq;
use atlas_smr_core::message::SystemMessage;
use febft_pbft_consensus::bft::message::{ConsensusMessageKind, PBFTMessage};

use crate::chaos::{EdgeRule, GateObserver, Predicate};
use crate::composition::AppData;
use crate::composition::febft::ProtocolDataType;
use crate::oracle::VoteLog;

/// Deserialize a wire message's payload as the febft protocol message, if it carries one.
/// Returns `None` for non-`Protocol`-module messages, undeserializable payloads, or
/// non-consensus/non-protocol variants.
fn as_pbft(msg: &WireMessage) -> Option<PBFTMessage<SMRReq<AppData>>> {
    if *msg.message_module() != MessageModule::Protocol {
        return None;
    }
    let sys = deserialize_message::<&[u8], ProtocolDataType>(msg.payload()).ok()?;
    match sys {
        SystemMessage::ProtocolMessage(prot) => Some(prot.into_inner()),
        _ => None,
    }
}

/// Which febft consensus phase a message belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsensusPhase {
    PrePrepare,
    Prepare,
    Commit,
}

fn phase_of(kind: &ConsensusMessageKind<SMRReq<AppData>>) -> ConsensusPhase {
    match kind {
        ConsensusMessageKind::PrePrepare(_) => ConsensusPhase::PrePrepare,
        ConsensusMessageKind::Prepare(_) => ConsensusPhase::Prepare,
        ConsensusMessageKind::Commit(_) => ConsensusPhase::Commit,
    }
}

/// Predicate: this message is a consensus message of `phase`, optionally for a specific
/// `seq` (`None` = any sequence).
pub fn consensus(phase: ConsensusPhase, seq: Option<SeqNo>) -> Predicate {
    Arc::new(move |msg: &WireMessage| match as_pbft(msg) {
        Some(PBFTMessage::Consensus(c)) => {
            phase_of(c.kind()) == phase && seq.is_none_or(|s| c.sequence_number() == s)
        }
        _ => false,
    })
}

/// Predicate: the leader's outbound COMMIT — the `prepare_vote_quorum` hook. This is the
/// key phase for the "leader crashes after a quorum of votes but before commit is
/// observed" scenario. (febft broadcasts COMMIT per-replica; this fires on whichever node
/// reaches quorum and is about to broadcast its COMMIT.)
pub fn commit(seq: Option<SeqNo>) -> Predicate {
    consensus(ConsensusPhase::Commit, seq)
}

/// Predicate: an outbound PRE-PREPARE (the leader proposing a batch).
pub fn pre_prepare(seq: Option<SeqNo>) -> Predicate {
    consensus(ConsensusPhase::PrePrepare, seq)
}

/// Predicate: an outbound PREPARE.
pub fn prepare(seq: Option<SeqNo>) -> Predicate {
    consensus(ConsensusPhase::Prepare, seq)
}

/// Predicate: any view-change (STOP / STOP-DATA / SYNC) message.
pub fn view_change() -> Predicate {
    Arc::new(|msg: &WireMessage| matches!(as_pbft(msg), Some(PBFTMessage::ViewChange(_))))
}

/// Build the inline-gate rule for "crash the leader just as it is about to broadcast its
/// COMMIT" (the `AfterVoteQuorumBeforeCommitBroadcast` sub-case, F1): the matching COMMIT
/// is the message that triggers the crash, so it — and every other COMMIT copy the leader
/// has in flight — is dropped before any peer sees it. `seq = None` matches the leader's
/// next COMMIT for any sequence.
pub fn crash_leader_before_commit_broadcast(leader: NodeId, seq: Option<SeqNo>) -> EdgeRule {
    EdgeRule::crash_on(leader, Some(MessageModule::Protocol), commit(seq))
}

/// A gate observer that records every PREPARE/COMMIT vote each node casts into `log`,
/// keyed by `(node, seq, phase)` with the batch digest — so equivocation (two different
/// digests for the same key) is detectable. PRE-PREPAREs are skipped (no single vote
/// digest). Requires distinct per-node keys (F6) to be meaningful.
pub fn record_votes(log: VoteLog) -> GateObserver {
    Arc::new(move |from, _to, msg: &WireMessage| {
        if let Some(PBFTMessage::Consensus(c)) = as_pbft(msg) {
            let (phase, digest) = match c.kind() {
                ConsensusMessageKind::Prepare(d) => ("prepare", format!("{d:?}")),
                ConsensusMessageKind::Commit(d) => ("commit", format!("{d:?}")),
                ConsensusMessageKind::PrePrepare(_) => return,
            };
            log.record(from, i64::from(u32::from(c.sequence_number())), phase, digest);
        }
    })
}

