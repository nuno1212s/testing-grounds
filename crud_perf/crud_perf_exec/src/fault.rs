//! Test-only fault injection: restricts where one client's first sends go, to exercise the
//! replicas' own request timeouts. Off unless `CLIENT_FAULT_SENDS_TO` is leader|non_leader|nobody.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use atlas_client::client::{Client, ClientType};
use atlas_common::node_id::NodeId;
use atlas_common::ordering::SeqNo;
use atlas_communication::stub::RegularNetworkStub;
use atlas_core::messages::RequestMessage;
use atlas_core::reconfiguration_protocol::ReconfigurationProtocol;
use atlas_smr_application::serialize::ApplicationData;
use atlas_smr_core::message::OrderableMessage;
use atlas_smr_core::serialize::{SMRSysMessage, SMRSysMsg};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SendTo {
    /// The view's leader (lowest replica id) only.
    Leader,
    /// One non-leader replica (second-lowest replica id) only.
    NonLeader,
    Nobody,
}

struct Fault {
    client: NodeId,
    to: SendTo,
    remaining: AtomicU64,
}

fn fault() -> Option<&'static Fault> {
    static FAULT: OnceLock<Option<Fault>> = OnceLock::new();

    FAULT
        .get_or_init(|| {
            let to = match std::env::var("CLIENT_FAULT_SENDS_TO").as_deref() {
                Err(_) | Ok("") | Ok("all") => return None,
                Ok("leader") => SendTo::Leader,
                Ok("non_leader") => SendTo::NonLeader,
                Ok("nobody") => SendTo::Nobody,
                Ok(v) => {
                    panic!("CLIENT_FAULT_SENDS_TO must be leader|non_leader|nobody, got {v:?}")
                }
            };

            let number = |name: &str| -> u64 {
                let v = std::env::var(name).unwrap_or_else(|_| panic!("{name} is required"));
                v.parse()
                    .unwrap_or_else(|_| panic!("{name} must be an integer, got {v:?}"))
            };

            Some(Fault {
                client: NodeId(number("CLIENT_FAULT_CLIENT") as u32),
                to,
                remaining: AtomicU64::new(number("CLIENT_FAULT_SENDS")),
            })
        })
        .as_ref()
}

fn restrict(to: SendTo, mut quorum: Vec<NodeId>) -> Vec<NodeId> {
    quorum.sort();

    match to {
        SendTo::Leader => quorum.truncate(1),
        SendTo::NonLeader => quorum = quorum.into_iter().skip(1).take(1).collect(),
        SendTo::Nobody => quorum.clear(),
    }

    quorum
}

/// `Ordered`, except that the faulty client's first `CLIENT_FAULT_SENDS` sends are restricted.
pub struct FaultyOrdered;

impl<RF, D, NT> ClientType<RF, D, NT> for FaultyOrdered
where
    D: ApplicationData + 'static,
    RF: ReconfigurationProtocol + 'static,
    NT: RegularNetworkStub<SMRSysMsg<D>> + 'static,
{
    fn init_request(
        session_id: SeqNo,
        operation_id: SeqNo,
        operation: D::Request,
    ) -> SMRSysMessage<D> {
        OrderableMessage::OrderedRequest(RequestMessage::new(session_id, operation_id, operation))
    }

    type Iter = std::vec::IntoIter<NodeId>;

    fn init_targets(client: &Client<RF, D, NT>) -> (Self::Iter, usize) {
        let mut quorum = client.get_quorum_view();

        if let Some(f) = fault().filter(|f| f.client == client.id()) {
            let took = f
                .remaining
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1))
                .is_ok();

            if took {
                quorum = restrict(f.to, quorum);
            }
        }

        let len = quorum.len();

        (quorum.into_iter(), len)
    }

    fn needed_responses(client: &Client<RF, D, NT>) -> usize {
        client.current_f() + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quorum() -> Vec<NodeId> {
        [2, 0, 3, 1].into_iter().map(NodeId).collect()
    }

    #[test]
    fn leader_is_the_lowest_id_and_non_leader_the_next() {
        assert_eq!(restrict(SendTo::Leader, quorum()), vec![NodeId(0)]);
        assert_eq!(restrict(SendTo::NonLeader, quorum()), vec![NodeId(1)]);
        assert!(restrict(SendTo::Nobody, quorum()).is_empty());
    }
}
