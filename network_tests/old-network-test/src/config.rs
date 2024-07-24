use config::FileFormat::Toml;
use config::{Config, File};
use getset::CopyGetters;
use serde::Deserialize;

#[derive(Deserialize, CopyGetters, Clone)]
pub(super) struct TestConfiguration {
    #[get_copy = "pub"]
    node_count: usize,
    #[get_copy = "pub"]
    concurrent_rqs_per_node: usize,
    #[get_copy = "pub"]
    messages_to_send: usize,
    #[get_copy = "pub"]
    message_size: usize,
    #[get_copy = "pub"]
    base_port: u16,
}

pub fn get_configuration_for_test() -> TestConfiguration {
    let config_reader = Config::builder()
        .add_source(File::new("config/test_config.toml", Toml))
        .build()
        .expect("Failed to build configuration reader");

    config_reader
        .try_deserialize()
        .expect("Failed to deserialize configuration")
}
