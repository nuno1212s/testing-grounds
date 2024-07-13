use std::collections::{BTreeMap, HashMap};

use clap::{Parser, Subcommand};
use getset::{CopyGetters, Getters};

use crate::config::{MachineConfig, MachineRestrictions};
use crate::writer::docker_compose::DockerComposeWriter;
use crate::writer::placeholders::ReplaceFormatter;

mod config;

mod allocators {
    pub mod round_robin;
}

mod writer {
    pub mod docker_compose;
    pub mod placeholders;
}

/// The scenario to create a test scenario.
#[derive(Parser, Debug, Clone, CopyGetters, Getters)]
#[command(
    author = "Nuno Neto",
    version,
    about = "A docker compose file generation utility for the automated deployment and execution of microbenchmarks."
)]
pub struct TestRunScenario {
    #[arg(long, default_value_t = 4, help = "The number of replicas to deploy.")]
    #[get_copy = "pub"]
    replica_count: usize,
    #[arg(long, default_value_t = 1, help = "The number of clients to deploy.")]
    #[get_copy = "pub"]
    client_count: usize,
    #[arg(long, default_value_t = 1000, help = "Where to start assigning the client node ids.")]
    #[get_copy = "pub"]
    first_cli_id: usize,
    #[arg(
        long,
        default_value_t = 0,
        help = "The size of the request (in bytes)."
    )]
    #[get_copy = "pub"]
    request_size: usize,
    #[arg(long, default_value_t = 0, help = "The size of the reply (in bytes).")]
    #[get_copy = "pub"]
    reply_size: usize,
    #[arg(
        short,
        long,
        default_value_t = 0,
        help = "The size of the state (in bytes)."
    )]
    #[get_copy = "pub"]
    state_size: usize,
    #[command(subcommand)]
    #[get = "pub"]
    allocator: AllocatorType,
}
/// The current state of allocations in the system.
#[derive(Getters)]
struct AllocatedState {
    #[get = "pub"]
    last_allocated_node: Option<String>,
    #[get = "pub"]
    cluster_state: BTreeMap<String, NodeState>,
}

/// The state of the node
#[derive(CopyGetters, Default)]
#[get_copy = "pub"]
struct NodeState {
    allocated_replicas: usize,
    allocated_clients: usize,
}

/// The allocator for the nodes.
trait NodeAllocator {
    // Get the node where to allocate the next replica/client
    fn get_node_to_allocate(
        &self,
        is_client: bool,
        current_state: &mut AllocatedState,
        restrictions: Option<&HashMap<String, MachineRestrictions>>,
    ) -> String;
}

#[derive(Debug, Clone, Subcommand)]
enum AllocatorType {
    #[command()]
    RoundRobin,
}

fn main() {
    let test_config = TestRunScenario::parse();

    let cluster_config =
        config::get_cluster_config().expect("Failed to parse cluster configuration");

    println!("Initializing with configuration: {:?}.", test_config);
    println!("Cluster configuration: {:?}", cluster_config);

    let allocator: Box<dyn NodeAllocator> = match test_config.allocator() {
        AllocatorType::RoundRobin => {
            Box::new(allocators::round_robin::RoundRobinAllocator::default())
        }
    };

    let mut current_state = AllocatedState {
        last_allocated_node: None,
        cluster_state: cluster_config
            .machine_list()
            .iter()
            .map(|node| (node.name().clone(), NodeState::default()))
            .collect(),
    };

    let restriction_map: HashMap<String, MachineRestrictions> = cluster_config
        .machine_list()
        .iter()
        .filter(|node| node.restrictions().is_some())
        .map(|node| (node.name().clone(), node.restrictions().clone().unwrap()))
        .collect();

    let config_map: HashMap<String, MachineConfig> = cluster_config
        .machine_list()
        .iter()
        .map(|node| (node.name().clone(), node.clone()))
        .collect();

    let formatter = ReplaceFormatter::new().expect("Failed to initialize placeholder replacer");

    let mut doc_writer =
        DockerComposeWriter::new(formatter).expect("Failed to initialize docker compose writer");

    for replica_id in 0..test_config.replica_count() {
        let node =
            allocator.get_node_to_allocate(false, &mut current_state, Some(&restriction_map));

        println!("Replica {} allocated to node {}", replica_id, node);

        let node_config = config_map.get(&node).expect("Allocated to unknown node");

        doc_writer
            .allocate_to_node(node_config, replica_id, false)
            .expect("Failed to allocate replica");
    }
    
    for client_id in test_config.first_cli_id()..test_config.first_cli_id() + test_config.client_count() {
        let node =
            allocator.get_node_to_allocate(true, &mut current_state, Some(&restriction_map));

        println!("Client {} allocated to node {}", client_id, node);

        let node_config = config_map.get(&node).expect("Allocated to unknown node");

        doc_writer
            .allocate_to_node(node_config, client_id, true)
            .expect("Failed to allocate client");
    }
}

trait DocumentWriter {
    // Allocate a replica or client node to a machine
    fn allocate_to_node(
        &mut self,
        machine: &MachineConfig,
        node_id: usize,
        is_client: bool,
    ) -> anyhow::Result<()>;
}
