use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};
use toml::{map::Map, Value};

use crate::ExternalCommand;

#[derive(Clone, Debug)]
pub struct ModSet {
    writable: bool,
    should_run_pre_commands: bool,
    command: Option<ExternalCommand>,
    mods: Vec<String>,
    mod_sets: HashMap<String, ModSet>,
    root_path: PathBuf,
}

impl ModSet {
    pub fn from_config(
        set_id: &str,
        set_config: &Map<String, Value>,
        game_id: String,
        game_config: &Map<String, Value>,
        root_path: PathBuf,
        visited: &mut HashSet<String>,
    ) -> Result<Self, String> {
        let mod_array = match set_config.get("mods") {
            Some(value) => value.as_array().ok_or(format!(
                "Failed getting array 'mods' for configuration set `{}` of game `{}`",
                set_id, game_id
            ))?,
            None => {
                return Err(format!(
                    "Missing 'mods' section for configuration set `{}` of game `{}`",
                    set_id, game_id
                ));
            }
        };

        if mod_array.is_empty() {
            return Err(format!(
                "Array 'mods' in configuration set `{}` of game `{}` is empty",
                set_id, game_id
            ));
        };

        let mut added_mods: Vec<String> = vec![];
        let mut mod_sets = HashMap::new();
        for mod_array_item in mod_array {
            if !mod_array_item.is_str() {
                return Err(format!(
                    "Array entry in configuration set `{}` of game `{}` is not a string",
                    set_id, game_id
                ));
            };

            let mod_name = mod_array_item.as_str().ok_or(format!(
                "Failed to convert array entry in configuration set `{}` of game `{}` into string",
                set_id, game_id
            ))?;

            match game_config.get(mod_name) {
                Some(config) => {
                    let sub_table = config.as_table().ok_or(format!(
                        "Failed to get set `{}` of game `{}`",
                        mod_name, game_id
                    ))?;

                    if visited.contains(&mod_name.to_string()) {
                        return Err(format!("Recursion detected in set `{}` of game `{}`: it already contains `{}`.", set_id, game_id, mod_name));
                    }
                    visited.insert(mod_name.to_string());

                    let sub_set = match ModSet::from_config(
                        mod_name,
                        sub_table,
                        game_id.clone(),
                        game_config,
                        root_path.clone(),
                        visited,
                    ) {
                        Ok(set) => set,
                        Err(error) => {
                            return Err(format!(
                                "Failed to add mod set `{}` of game `{}`: {}",
                                set_id, game_id, error
                            ));
                        }
                    };

                    visited.remove(mod_name);
                    mod_sets.insert(mod_name.to_string(), sub_set);
                }
                None => {
                    let mod_path = root_path.join(mod_name);
                    match mod_path.try_exists() {
                        Ok(exists) => {
                            if !exists {
                                return Err(format!(
                                    "Mod set folder `{}` of game `{}` does not exist",
                                    mod_path.display(),
                                    game_id
                                ));
                            }
                        }
                        Err(error) => {
                            return Err(format!(
                                "Mod set folder `{}` of game `{}` could not be accessed: {}",
                                mod_path.display(),
                                game_id,
                                error
                            ));
                        }
                    }
                }
            }
            added_mods.push(mod_name.to_string());
        }

        let writable = match set_config.get("writable") {
            Some(value) => value.as_bool().ok_or(format!(
                "Could not convert `writable` to bool in mod set `{}` in game `{}`",
                set_id, game_id
            ))?,
            None => false,
        };

        let should_run_pre_commands = match set_config.get("run_pre_command") {
            Some(value) => value.as_bool().ok_or(format!(
                "Could not convert `should_run_pre_commands` to bool in mod set `{}` in game `{}`",
                set_id, game_id
            ))?,
            None => false,
        };

        let command_name: Option<&str> = match set_config.get("command") {
            Some(value) => Some(value.as_str().ok_or(format!(
                "Could not convert `command` to string in mod set `{}` in game `{}`",
                set_id, game_id
            ))?),
            None => None,
        };

        let mut command = None;
        if command_name.is_some() {
            let command_table = match game_config.get(command_name.unwrap()) {
                Some(value) => value.as_table().ok_or(format!(
                    "Could not convert `{}` to table in game `{}`",
                    command_name.unwrap(),
                    game_id
                ))?,
                None => {
                    return Err(format!(
                        "No such command `{}` in game `{}`",
                        command_name.unwrap(),
                        game_id
                    ));
                }
            };

            command = Some(
                ExternalCommand::from_config(
                    game_id.clone(),
                    command_name.unwrap().to_string(),
                    command_table,
                )
                .or_else(|error| {
                    return Err(format!(
                        "Could not parse command `{}` in game `{}`: {}",
                        command_name.unwrap(),
                        game_id,
                        error
                    ));
                })?,
            );
        }

        return Ok(ModSet {
            writable,
            should_run_pre_commands,
            command,
            mods: added_mods.clone(),
            mod_sets,
            root_path,
        });
    }

    pub fn get_commands(&self) -> Vec<&ExternalCommand> {
        let mut list: Vec<&ExternalCommand> = vec![];

        for mod_set in self.mod_sets.values() {
            list.append(&mut mod_set.get_commands().clone());
        }

        if let Some(cmd) = self.command.clone() {
            if !list.iter().any(|c| c.id == cmd.id) {
                list.push(&self.command.as_ref().unwrap());
            }
        }

        return list;
    }

    pub fn get_mount_string(&self, mount_string: &mut String) -> () {
        for mod_name in &self.mods {
            match self.mod_sets.get(mod_name) {
                Some(set) => {
                    set.get_mount_string(mount_string);
                }
                None => {
                    let mod_path = self
                        .root_path
                        .join(&mod_name)
                        .to_str()
                        .expect("Unable to get string version of PathBuf.")
                        .replace(":", r#"\:"#);

                    if !mount_string.contains(&mod_path) {
                        // mount_string = &mut format!("{}:{}", &mount_string, mod_path);
                        mount_string.push_str(&format!(":{}", mod_path));
                    }
                }
            }
        }

        *mount_string = mount_string.trim_start_matches(':').to_string();
    }

    pub fn should_run_pre_commands(&self) -> bool {
        if self.should_run_pre_commands {
            return true;
        }

        for mod_set in self.mod_sets.values() {
            if mod_set.should_run_pre_commands() {
                return true;
            }
        }

        return false;
    }

    pub fn should_be_writable(&self) -> bool {
        if self.writable {
            return true;
        }

        for mod_set in self.mod_sets.values() {
            if mod_set.should_be_writable() {
                return true;
            }
        }

        return false;
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use toml::Table;

    use super::*;

    #[test]
    fn parsing() {
        let game_config = r#"
        ["set1"]
        writable = true
        run_pre_command = true
        command = "asd"
        mods = ["set2", "mod 1", "mod 2"]

        ["set2"]
        mods = ["mod 1", "mod: 3"]

        ["asd"]
        command = [
            "echo",
            "asd"
        ]
        "#
        .parse::<Table>()
        .unwrap();

        let set_config = game_config.get("set1").unwrap().as_table().unwrap().clone();
        let root_path = PathBuf::from(String::from("test/mod: root"))
            .canonicalize()
            .unwrap();

        assert!(ModSet::from_config(
            "set1",
            &set_config,
            "test_game".to_owned(),
            &game_config,
            root_path,
            &mut HashSet::new()
        )
        .is_ok());
    }

    #[test]
    fn mods_recursion() {
        let game_config = r#"
        ["set1"]
        mods = ["set2"]
        ["set2"]
        mods = ["set1"]
        "#
        .parse::<Table>()
        .unwrap();

        let set_config = game_config.get("set1").unwrap().as_table().unwrap().clone();
        let root_path = PathBuf::from(String::from("test/mod: root"))
            .canonicalize()
            .unwrap();

        assert!(ModSet::from_config(
            "set1",
            &set_config,
            "test_game".to_owned(),
            &game_config,
            root_path,
            &mut HashSet::new()
        )
        .is_err());
    }

    #[test]
    fn mods_malformed() {
        let game_config = r#"
        ["set1"]
        mods = [0]
        "#
        .parse::<Table>()
        .unwrap();

        let set_config = game_config.get("set1").unwrap().as_table().unwrap().clone();
        let root_path = PathBuf::from(String::from("test/mod: root"))
            .canonicalize()
            .unwrap();

        assert!(ModSet::from_config(
            "set1",
            &set_config,
            "test_game".to_owned(),
            &game_config,
            root_path,
            &mut HashSet::new()
        )
        .is_err());
    }

    #[test]
    fn writable_malformed() {
        let game_config = r#"
        ["set1"]
        writable = "asd"
        mods = ["set2"]
        "#
        .parse::<Table>()
        .unwrap();

        let set_config = game_config.get("set1").unwrap().as_table().unwrap().clone();
        let root_path = PathBuf::from(String::from("test/mod: root"))
            .canonicalize()
            .unwrap();

        assert!(ModSet::from_config(
            "set1",
            &set_config,
            "test_game".to_owned(),
            &game_config,
            root_path,
            &mut HashSet::new()
        )
        .is_err());
    }

    #[test]
    fn mods_empty() {
        let game_config = r#"
        ["set1"]
        mods = []
        "#
        .parse::<Table>()
        .unwrap();

        let set_config = game_config.get("set1").unwrap().as_table().unwrap().clone();
        let root_path = PathBuf::from(String::from("test/mod: root"))
            .canonicalize()
            .unwrap();

        assert!(ModSet::from_config(
            "set1",
            &set_config,
            "test_game".to_owned(),
            &game_config,
            root_path,
            &mut HashSet::new()
        )
        .is_err());
    }

    #[test]
    fn run_pre_command_malformed() {
        let game_config = r#"
        ["set1"]
        run_pre_command = "asd"
        mods = ["set2"]
        "#
        .parse::<Table>()
        .unwrap();

        let set_config = game_config.get("set1").unwrap().as_table().unwrap().clone();
        let root_path = PathBuf::from(String::from("test/mod: root"))
            .canonicalize()
            .unwrap();

        assert!(ModSet::from_config(
            "set1",
            &set_config,
            "test_game".to_owned(),
            &game_config,
            root_path,
            &mut HashSet::new()
        )
        .is_err());
    }

    #[test]
    fn special_command_malformed() {
        let game_config = r#"
        ["set1"]
        command = 0
        mods = ["set2"]

        ["asd"]
        command = [
            "echo",
            "special_command_test"
        ]
        "#
        .parse::<Table>()
        .unwrap();

        let set_config = game_config.get("set1").unwrap().as_table().unwrap().clone();
        let root_path = PathBuf::from(String::from("test/mod: root"))
            .canonicalize()
            .unwrap();

        assert!(ModSet::from_config(
            "set1",
            &set_config,
            "test_game".to_owned(),
            &game_config,
            root_path,
            &mut HashSet::new()
        )
        .is_err());
    }

    #[test]
    fn special_command_unavailable() {
        let game_config = r#"
        ["set1"]
        command = "asd"
        mods = ["set2"]
        "#
        .parse::<Table>()
        .unwrap();

        let set_config = game_config.get("set1").unwrap().as_table().unwrap().clone();
        let root_path = PathBuf::from(String::from("test/mod: root"))
            .canonicalize()
            .unwrap();

        assert!(ModSet::from_config(
            "set1",
            &set_config,
            "test_game".to_owned(),
            &game_config,
            root_path,
            &mut HashSet::new()
        )
        .is_err());
    }

    #[test]
    fn get_commands() {
        let game_config = r#"
        ["set1"]
        command = "asd"
        mods = ["set2"]

        ["set2"]
        command = "dsa"
        mods = ["mod 1"]

        ["asd"]
        command = [
            "echo",
            "asd"
        ]

        ["dsa"]
        command = [
            "ls",
            "test"
        ]
        "#
        .parse::<Table>()
        .unwrap();

        let set_config = game_config.get("set1").unwrap().as_table().unwrap().clone();
        let root_path = PathBuf::from(String::from("test/mod: root"))
            .canonicalize()
            .unwrap();
        let mod_set = ModSet::from_config(
            "set1",
            &set_config,
            "test_game".to_owned(),
            &game_config,
            root_path,
            &mut HashSet::new(),
        )
        .unwrap();

        let asd_command = ExternalCommand::from_config(
            "test_game".to_string(),
            "asd".to_string(),
            &game_config.get("asd").unwrap().as_table().unwrap(),
        )
        .unwrap();
        let dsa_command = ExternalCommand::from_config(
            "test_game".to_string(),
            "dsa".to_string(),
            &game_config.get("dsa").unwrap().as_table().unwrap(),
        )
        .unwrap();

        let mut commands: Vec<&ExternalCommand> = vec![];
        commands.push(&dsa_command);
        commands.push(&asd_command);

        for (i, command) in mod_set.get_commands().iter().enumerate() {
            assert_eq!(command.id, commands[i].id);
        }
    }

    #[test]
    fn get_mount_string() {
        let game_config = r#"
        ["set1"]
        mods = ["set2", "mod 1", "mod 2"]
        ["set2"]
        mods = ["mod 1", "mod: 3"]
        "#
        .parse::<Table>()
        .unwrap();

        let set_config = game_config.get("set1").unwrap().as_table().unwrap().clone();
        let root_path = PathBuf::from(String::from("test/mod: root"))
            .canonicalize()
            .unwrap();
        let mod1_path = root_path
            .join("mod 1")
            .to_str()
            .unwrap()
            .replace(":", r#"\:"#);
        let mod2_path = root_path
            .join("mod 2")
            .to_str()
            .unwrap()
            .replace(":", r#"\:"#);
        let mod3_path = root_path
            .join("mod: 3")
            .to_str()
            .unwrap()
            .replace(":", r#"\:"#);
        let mnt_string = format!("{}:{}:{}", mod1_path, mod3_path, mod2_path);

        let mut mount_string = "".to_owned();
        ModSet::from_config(
            "set1",
            &set_config,
            "test_game".to_owned(),
            &game_config,
            root_path,
            &mut HashSet::new(),
        )
        .unwrap()
        .get_mount_string(&mut mount_string);

        assert_eq!(mount_string, mnt_string);
    }
}
