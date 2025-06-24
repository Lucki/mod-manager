use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Deserialize, Clone)]
pub struct MainConfig {
    pub editor: Option<String>,
    pub default: Option<DefaultConfig>,
    pub template: Option<TemplateConfig>,
}

#[derive(Deserialize, Clone)]
pub struct DefaultConfig {
    pub game_root_path: Option<PathBuf>,
    pub mod_root_path: Option<PathBuf>,
}

#[derive(Deserialize, Clone)]
pub struct TemplateConfig {
    pub path: Option<String>,
    pub mod_root_path: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct GameConfig {
    pub active: Option<String>,
    pub path: Option<PathBuf>,
    pub mod_root_path: Option<PathBuf>,
    pub writable: Option<bool>,
    pub run_pre_command: Option<bool>,
    pub pre_command: Option<Vec<CommandConfig>>,
    pub commands: Option<SpecificCommandsConfig>,

    #[serde(flatten)]
    pub sets: HashMap<String, ModSetConfig>,
}

#[derive(Deserialize, Clone)]
pub struct ModSetConfig {
    pub run_pre_command: Option<bool>,
    pub command: Option<String>,
    pub mods: Vec<String>,
    pub writable: Option<bool>,
    pub environment: Option<EnvironmentConfig>,
}

#[derive(Deserialize, Clone)]
pub struct EnvironmentConfig {
    #[serde(flatten)]
    pub variables: HashMap<String, String>,
}

#[derive(Deserialize, Clone)]
pub struct CommandConfig {
    pub wait_for_exit: Option<bool>,
    pub delay_after: Option<u64>,
    pub command: Vec<String>,
    pub environment: Option<EnvironmentConfig>,
}

#[derive(Deserialize, Clone)]
pub struct SpecificCommandsConfig {
    #[serde(flatten)]
    pub named_commands: HashMap<String, CommandConfig>,
}

// pub fn parse_toml_table<T>(table: toml::Table) -> T {
//     let asd: T = toml::from_str(&toml::to_string(&table).unwrap()).unwrap();
//     asd
// }
