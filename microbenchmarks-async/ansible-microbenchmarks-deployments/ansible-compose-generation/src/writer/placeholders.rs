use crate::config::MachineConfig;
use crate::writer::docker_compose::Formatter;
use config::Config;
use getset::Getters;
use linked_hash_map::LinkedHashMap;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Getters, Clone, Debug)]
pub struct RegisteredPlaceHoldersConfig {
    placeholders: HashMap<String, String>,
}

#[derive(Getters, Clone, Debug)]
pub struct RegisteredPlaceHolders {
    #[get = "pub"]
    placeholders: LinkedHashMap<String, String>,
}

pub fn build_placeholders_for_machine(
    config: &MachineConfig,
    node: usize,
    is_client: bool,
) -> HashMap<String, String> {
    let mut placeholders = HashMap::new();

    placeholders.insert("machine".to_string(), config.name().clone());
    placeholders.insert("machineIp".to_string(), config.machine_ip().clone());
    placeholders.insert("nodeId".to_string(), format!("{}", node));
    placeholders.insert(
        "nodeType".to_string(),
        (if is_client { "Client" } else { "Replica" }).to_string(),
    );
    placeholders.insert("nodePort".to_string(), format!("{}", config.base_port() as usize + node));

    placeholders
}

pub fn parse_placeholder_config() -> anyhow::Result<RegisteredPlaceHolders> {
    let config = Config::builder()
        .add_source(config::File::new(
            "config/placeholders.toml",
            config::FileFormat::Toml,
        ))
        .build()?;

    let placeholder_config: RegisteredPlaceHoldersConfig = config.try_deserialize()?;

    Ok(placeholder_config.into())
}

impl From<RegisteredPlaceHoldersConfig> for RegisteredPlaceHolders {
    fn from(value: RegisteredPlaceHoldersConfig) -> Self {
        let mut map: LinkedHashMap<String, String> = Default::default();

        value.placeholders.into_iter().for_each(|(key, value)| {
            map.insert(key, value);
        });

        Self { placeholders: map }
    }
}

pub(crate) struct ReplaceFormatter {
    replace_placeholders: RegisteredPlaceHolders,
}

impl ReplaceFormatter {
    pub(crate) fn new() -> anyhow::Result<Self> {
        let placeholders = parse_placeholder_config()?;

        Ok(Self {
            replace_placeholders: placeholders,
        })
    }
}

impl Formatter for ReplaceFormatter {
    fn format_document(
        &self,
        document: &String,
        override_formats: Option<HashMap<String, String>>,
    ) -> String {
        let mut document = document.clone();

        self.replace_placeholders
            .placeholders()
            .iter()
            .for_each(|(key, value)| {
                let value = override_formats
                    .as_ref()
                    .and_then(|overrides| overrides.get(key))
                    .unwrap_or(value);

                document = document.replace(&format!("${{{}}}", key), value);
            });

        if let Some(overrides) = override_formats.as_ref() {
            overrides
                .iter()
                .filter(|(key, _)| !self.replace_placeholders.placeholders().contains_key(*key))
                .for_each(|(key, value)| {
                    document = document.replace(&format!("${{{}}}", key), value);
                });
        }

        document
    }
}
