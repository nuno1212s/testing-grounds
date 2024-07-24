use config::FileFormat::Toml;
use config::{Config, File};
use getset::CopyGetters;
use serde::Deserialize;

#[derive(Deserialize, CopyGetters, Clone)]
#[get_copy = "pub"]
pub(super) struct TestConfiguration {
    node_count: usize,
    concurrent_rqs_per_node: usize,
    messages_to_send: usize,
    message_size: usize,
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
