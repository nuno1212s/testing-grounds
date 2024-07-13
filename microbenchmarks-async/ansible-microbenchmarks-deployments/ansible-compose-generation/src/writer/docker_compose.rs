use crate::config::MachineConfig;
use crate::writer::placeholders;
use crate::writer::placeholders::ReplaceFormatter;
use crate::DocumentWriter;
use config::Format;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;
use config::FileFormat::Toml;

pub struct DockerComposeWriter<T> {
    base_docker_compose: String,
    replica_docker_compose: String,
    client_docker_compose: String,
    output_files_dir: PathBuf,

    formatter: T,

    files_for_replicas: HashMap<String, BufWriter<File>>,
    files_for_clients: HashMap<String, BufWriter<File>>,
}

fn read_path_to_string(path: PathBuf) -> anyhow::Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    let mut contents = String::new();
    reader.read_to_string(&mut contents)?;

    Ok(contents)
}

const DOCKER_FILE_NAME: &str = "docker-compose.yml";
const DOCKER_CLIENT_NAME: &str = "cli-docker-compose.yml";

fn docker_name_for(node: &str, is_client: bool) -> String {
    format!("{}-{}", node, if is_client { DOCKER_CLIENT_NAME } else { DOCKER_FILE_NAME })
}

impl<T> DockerComposeWriter<T> {
    pub fn new(formatter: T) -> anyhow::Result<Self> {
        let config = config::Config::builder()
            .add_source(config::File::new("config/docker-default.toml", Toml))
            .add_source(config::File::new("config/docker-local.toml", Toml).required(false))
            .add_source(config::Environment::with_prefix("COMPOSE"))
            .build()?;

        let DockerComposeConfig {
            docker_compose_base: docker_base_file,
            docker_compose_replica: replica_docker_file,
            docker_compose_client: client_docker_file,
            output_directory: output_files_dir,
        } = config.try_deserialize()?;

        if output_files_dir.exists() && !output_files_dir.is_dir() {
            return Err(anyhow::anyhow!("Output files directory is not a directory"));
        } else if !output_files_dir.exists() {
            std::fs::create_dir_all(&output_files_dir)?;
        }

        Ok(Self {
            base_docker_compose: read_path_to_string(docker_base_file)?,
            replica_docker_compose: read_path_to_string(replica_docker_file)?,
            client_docker_compose: read_path_to_string(client_docker_file)?,
            output_files_dir,
            formatter,
            files_for_replicas: Default::default(),
            files_for_clients: Default::default(),
        })
    }

    fn write_initial_docker_compose(&self, file: &mut BufWriter<File>) -> anyhow::Result<()> {
        file.write_all(self.base_docker_compose.as_bytes())?;

        Ok(())
    }
}

impl<T> DocumentWriter for DockerComposeWriter<T>
where
    T: Formatter,
{
    fn allocate_to_node(
        &mut self,
        node: &MachineConfig,
        id: usize,
        is_client: bool,
    ) -> anyhow::Result<()> {
        let mut created = false;

        let target_map = if is_client {
            &mut self.files_for_clients
        } else {
            &mut self.files_for_replicas
        };

        let output_file = target_map.entry(node.name().clone()).or_insert_with(|| {
            let file_name = docker_name_for(node.name(), is_client);
            let file_path = self.output_files_dir.join(file_name);
            let file = File::create(file_path).expect("Cannot create file");
            created = true;
            BufWriter::new(file)
        });

        if created {
            output_file.write_all(self.base_docker_compose.as_bytes())?;
        }

        let to_format = if is_client {
            &self.client_docker_compose
        } else {
            &self.replica_docker_compose
        };

        let format_overrides = placeholders::build_placeholders_for_machine(node, id, is_client);

        let formatted_str = self
            .formatter
            .format_document(to_format, Some(format_overrides));

        output_file.write_all(formatted_str.as_bytes())?;

        Ok(())
    }
}

#[derive(Deserialize, Clone, Debug)]
pub(crate) struct DockerComposeConfig {
    docker_compose_base: PathBuf,
    docker_compose_replica: PathBuf,
    docker_compose_client: PathBuf,

    output_directory: PathBuf,
}

pub(super) trait Formatter {
    /// Format the provided document with the formatter's rules
    fn format_document(
        &self,
        document: &String,
        override_formats: Option<HashMap<String, String>>,
    ) -> String;
}
