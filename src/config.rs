use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Deserialize, Clone)]
pub(crate) struct MainConfig {
    pub(crate) editor: Option<String>,
    pub(crate) default: Option<DefaultConfig>,
    pub(crate) template: Option<TemplateConfig>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct DefaultConfig {
    pub(crate) game_root_path: Option<PathBuf>,
    pub(crate) mod_root_path: Option<PathBuf>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct TemplateConfig {
    pub(crate) path: Option<String>,
    pub(crate) mod_root_path: Option<String>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct GameConfig {
    /// Activate set with Some(set), deactivate overlays with None
    pub(crate) active: Option<String>,
    /// Path to game folder
    pub(crate) path: Option<PathBuf>,
    /// Path to folder containing mod folders
    pub(crate) mod_root_path: Option<PathBuf>,
    /// Mount overlay with an upper dir
    pub(crate) writable: Option<bool>,
    /// Run pre commands after mounting and before wrap
    pub(crate) run_pre_command: Option<bool>,
    /// List of generic pre commands
    pub(crate) pre_command: Option<Vec<CommandConfig>>,
    /// Named pre commands
    pub(crate) commands: Option<SpecificCommandsConfig>,

    /// List of all available mod sets
    #[serde(flatten)]
    pub(crate) sets: HashMap<String, ModSetConfig>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct ModSetConfig {
    /// Run pre commands after mounting and before wrap
    pub(crate) run_pre_command: Option<bool>,
    /// Run specific command after mounting and before wrap
    pub(crate) command: Option<String>,
    /// List of names of mods and sets to include
    pub(crate) mods: Vec<String>,
    /// Mount overlay with an upper dir
    pub(crate) writable: Option<bool>,
    /// Environment to set for wrap
    pub(crate) environment: Option<EnvironmentConfig>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct EnvironmentConfig {
    #[serde(flatten)]
    pub(crate) variables: HashMap<String, String>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct CommandConfig {
    /// Wait for command to exit before continuing
    pub(crate) wait_for_exit: Option<bool>,
    /// Delay following commands for x seconds
    pub(crate) delay_after: Option<u64>,
    /// Command argument array
    pub(crate) command: Vec<String>,
    /// Call command with these additional environment variables
    pub(crate) environment: Option<EnvironmentConfig>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct SpecificCommandsConfig {
    #[serde(flatten)]
    pub(crate) named_commands: HashMap<String, CommandConfig>,
}

// pub(crate) fn parse_toml_table<T>(table: toml::Table) -> T {
//     let asd: T = toml::from_str(&toml::to_string(&table).unwrap()).unwrap();
//     asd
// }
