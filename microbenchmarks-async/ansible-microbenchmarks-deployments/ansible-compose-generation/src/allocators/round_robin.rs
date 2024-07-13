use std::collections::{BTreeMap, HashMap};

use crate::{AllocatedState, MachineRestrictions, NodeAllocator};

#[derive(Default)]
pub(crate) struct RoundRobinAllocator;

impl NodeAllocator for RoundRobinAllocator {
    fn get_node_to_allocate(
        &self,
        is_client: bool,
        current_state: &mut AllocatedState,
        restrictions: Option<&HashMap<String, MachineRestrictions>>,
    ) -> String {
        if is_client {
            allocate_client(current_state, restrictions)
        } else {
            allocate_replica(current_state, restrictions)
        }
    }
}

fn allocate_client(
    current_state: &mut AllocatedState,
    restrictions: Option<&HashMap<String, MachineRestrictions>>,
) -> String {
    let nodes = current_state.cluster_state();

    let mut iterations = 0;

    loop {
        let next_node_round = if let Some(last_allocated_node) = current_state.last_allocated_node()
        {
            nodes
                .range(last_allocated_node.clone()..)
                .nth(1)
                .map(|(node, _)| node.clone())
                .unwrap_or_else(|| {
                    nodes
                        .first_key_value()
                        .expect("Cannot allocate on empty cluster")
                        .0
                        .clone()
                })
        } else {
            nodes
                .first_key_value()
                .expect("Cannot allocate on empty cluster")
                .0
                .clone()
        };

        if restrictions.is_some_and(|restrictions| {
            meets_restrictions(true, &next_node_round, restrictions, current_state)
        }) {
            current_state.last_allocated_node = Some(next_node_round.clone());

            return next_node_round;
        }

        iterations += 1;

        if iterations >= nodes.len() {
            panic!("Cannot allocate on any node");
        }
    }
}

fn allocate_replica(
    current_state: &mut AllocatedState,
    restrictions: Option<&HashMap<String, MachineRestrictions>>,
) -> String {
    let nodes = current_state.cluster_state();

    let mut iterations = 0;

    loop {
        let next_node_round = if let Some(last_allocated_node) = current_state.last_allocated_node()
        {
            nodes
                .range(last_allocated_node.clone()..)
                .nth(1)
                .map(|(node, _)| node.clone())
                .unwrap_or_else(|| {
                    nodes
                        .first_key_value()
                        .expect("Cannot allocate on empty cluster")
                        .0
                        .clone()
                })
        } else {
            nodes
                .first_key_value()
                .expect("Cannot allocate on empty cluster")
                .0
                .clone()
        };

        if restrictions.is_none()
            || restrictions.is_some_and(|restrictions| {
            meets_restrictions(true, &next_node_round, restrictions, current_state)
        })
        {
            current_state.last_allocated_node = Some(next_node_round.clone());

            return next_node_round;
        }

        iterations += 1;

        if iterations >= nodes.len() {
            panic!("Cannot allocate on any node");
        }
    }
}

fn meets_restrictions(
    client: bool,
    node: &str,
    restrictions: &HashMap<String, MachineRestrictions>,
    current_state: &AllocatedState,
) -> bool {
    let node_state = current_state.cluster_state().get(node).unwrap();
    let restrictions = restrictions.get(node);

    if let Some(restrictions) = restrictions {
        if client {
            node_state.allocated_clients() < restrictions.max_clients()
        } else {
            node_state.allocated_replicas() < restrictions.max_replicas()
        }
    } else {
        true
    }
}
