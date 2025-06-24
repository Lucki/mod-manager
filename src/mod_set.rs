use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use crate::ExternalCommand;
use crate::config::{GameConfig, ModSetConfig};

#[derive(Clone, Debug)]
pub struct ModSet {
    writable: bool,
    should_run_pre_commands: bool,
    command: Option<ExternalCommand>,
    mods: Vec<String>,
    mod_sets: HashMap<String, ModSet>,
    root_path: PathBuf,
    env: HashMap<String, String>,
}

impl ModSet {
    pub fn from_config(
        set_id: &str,
        set_config: &ModSetConfig,
        game_id: String,
        game_config: &GameConfig,
        root_path: PathBuf,
        visited: &mut HashSet<String>,
    ) -> Result<Self, String> {
        let mod_array = set_config.mods.clone();
        if mod_array.is_empty() {
            return Err(format!(
                "Array 'mods' in configuration set `{}` of game `{}` is empty",
                set_id, game_id
            ));
        };

        let mut added_mods: Vec<String> = vec![];
        let mut mod_sets = HashMap::new();
        for mod_name in mod_array {
            match game_config.sets.get(&mod_name) {
                Some(set_config) => {
                    if visited.contains(&mod_name) {
                        return Err(format!(
                            "Recursion detected in set `{}` of game `{}`: it already contains `{}`.",
                            set_id, game_id, mod_name
                        ));
                    }
                    visited.insert(mod_name.clone());

                    let sub_set = match ModSet::from_config(
                        &mod_name,
                        &set_config,
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

                    visited.remove(&mod_name);
                    mod_sets.insert(mod_name.clone(), sub_set);
                }
                None => {
                    let mod_path = root_path.join(&mod_name);
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
            added_mods.push(mod_name);
        }

        let writable = match set_config.writable {
            Some(value) => value,
            None => false,
        };

        let should_run_pre_commands = match set_config.run_pre_command {
            Some(value) => value,
            None => false,
        };

        let command = match &set_config.command {
            Some(name) => {
                let command_config = match &game_config.commands {
                    Some(named_commands_config) => {
                        match named_commands_config.named_commands.get(name) {
                            Some(value) => value,
                            None => {
                                return Err(format!(
                                    "No such command `{}` in game `{}`",
                                    name, game_id
                                ));
                            }
                        }
                    }
                    None => {
                        return Err(format!(
                            "Missing specific commands section in `{}`",
                            game_id
                        ));
                    }
                };

                Some(
                    ExternalCommand::from_config(
                        game_id.clone(),
                        name.to_string(),
                        &command_config,
                    )
                    .or_else(|error| {
                        return Err(format!(
                            "Could not parse command `{}` in game `{}`: {}",
                            name, game_id, error
                        ));
                    })?,
                )
            }
            None => None,
        };

        let env = match &set_config.environment {
            Some(environment_config) => environment_config.variables.clone(),
            None => HashMap::new(),
        };

        return Ok(ModSet {
            writable,
            should_run_pre_commands,
            command,
            mods: added_mods.clone(),
            mod_sets,
            root_path,
            env,
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
                    let mod_path = escape_special_mount_chars(
                        self.root_path
                            .join(&mod_name)
                            .to_str()
                            .expect("Unable to get string version of PathBuf.")
                            .to_owned(),
                    );

                    if !mount_string.contains(&mod_path) {
                        mount_string.push_str(&format!(",lowerdir+={}", mod_path));
                    }
                }
            }
        }

        // *mount_string = mount_string.trim_start_matches(':').to_string();
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

    /// Get a copy of all environment variables defined for this mod set tree
    pub fn get_environment(&self) -> HashMap<String, String> {
        let mut envs = self.env.clone();

        for mod_name in &self.mods {
            match self.mod_sets.get(mod_name) {
                Some(set) => {
                    for (k, v) in set.get_environment() {
                        envs.insert(k, v);
                    }
                }
                None => {}
            }
        }

        envs
    }

    /// Check if the current tree includes a specific mod
    pub fn contains(&self, id: String) -> bool {
        if self.mods.contains(&id) {
            return true;
        }

        for mod_set in self.mod_sets.values() {
            if mod_set.contains(id.clone()) {
                return true;
            }
        }

        false
    }
}

fn escape_special_mount_chars(string: String) -> String {
    string.replace(",", r#"\,"#)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn parsing() {
        let game_config: GameConfig = toml::from_str(
            r#"
        ["set1"]
        writable = true
        run_pre_command = true
        command = "asd"
        mods = ["set2", "mod 1", "mod 2"]

        ["set2"]
        mods = ["mod 1", "mod, 3"]

        [commands."asd"]
        command = [
            "echo",
            "asd"
        ]
        "#,
        )
        .unwrap();

        let set_config = game_config.sets.get("set1").unwrap();
        let root_path = PathBuf::from(String::from("test/mod, root"))
            .canonicalize()
            .unwrap();

        assert!(
            ModSet::from_config(
                "set1",
                &set_config,
                "test_game".to_owned(),
                &game_config,
                root_path,
                &mut HashSet::new()
            )
            .is_ok()
        );
    }

    #[test]
    fn mods_recursion() {
        let game_config: GameConfig = toml::from_str(
            r#"
            ["set1"]
            mods = ["set2"]
            ["set2"]
            mods = ["set1"]
            "#,
        )
        .unwrap();

        let set_config = game_config.sets.get("set1").unwrap();
        let root_path = PathBuf::from(String::from("test/mod, root"))
            .canonicalize()
            .unwrap();

        assert!(
            ModSet::from_config(
                "set1",
                &set_config,
                "test_game".to_owned(),
                &game_config,
                root_path,
                &mut HashSet::new()
            )
            .is_err()
        );
    }

    #[test]
    fn mods_empty() {
        let game_config: GameConfig = toml::from_str(
            r#"
        ["set1"]
        mods = []
        "#,
        )
        .unwrap();

        let set_config = game_config.sets.get("set1").unwrap();
        let root_path = PathBuf::from(String::from("test/mod, root"))
            .canonicalize()
            .unwrap();

        assert!(
            ModSet::from_config(
                "set1",
                &set_config,
                "test_game".to_owned(),
                &game_config,
                root_path,
                &mut HashSet::new()
            )
            .is_err()
        );
    }

    #[test]
    fn special_command_unavailable() {
        let game_config: GameConfig = toml::from_str(
            r#"
        ["set1"]
        command = "asd"
        mods = ["set2"]
        "#,
        )
        .unwrap();

        let set_config = game_config.sets.get("set1").unwrap();
        let root_path = PathBuf::from(String::from("test/mod, root"))
            .canonicalize()
            .unwrap();

        assert!(
            ModSet::from_config(
                "set1",
                &set_config,
                "test_game".to_owned(),
                &game_config,
                root_path,
                &mut HashSet::new()
            )
            .is_err()
        );
    }

    #[test]
    fn get_commands() {
        let game_config: GameConfig = toml::from_str(
            r#"
        ["set1"]
        command = "asd"
        mods = ["set2"]

        ["set2"]
        command = "dsa"
        mods = ["mod 1"]

        [commands."asd"]
        command = [
            "echo",
            "asd"
        ]

        [commands."dsa"]
        command = [
            "ls",
            "test"
        ]
        "#,
        )
        .unwrap();

        let set_config = game_config.sets.get("set1").unwrap();
        let root_path = PathBuf::from(String::from("test/mod, root"))
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
            game_config
                .commands
                .as_ref()
                .unwrap()
                .named_commands
                .get("asd")
                .unwrap(),
        )
        .unwrap();
        let dsa_command = ExternalCommand::from_config(
            "test_game".to_string(),
            "dsa".to_string(),
            game_config
                .commands
                .as_ref()
                .unwrap()
                .named_commands
                .get("dsa")
                .unwrap(),
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
        let game_config: GameConfig = toml::from_str(
            r#"
        ["set1"]
        mods = ["set2", "mod 1", "mod 2"]
        ["set2"]
        mods = ["mod 1", "mod, 3"]
        "#,
        )
        .unwrap();

        let set_config = game_config.sets.get("set1").unwrap();
        let root_path = PathBuf::from(String::from("test/mod, root"))
            .canonicalize()
            .unwrap();
        let mod1_path = root_path
            .join("mod 1")
            .to_str()
            .unwrap()
            .replace(",", r#"\,"#);
        let mod2_path = root_path
            .join("mod 2")
            .to_str()
            .unwrap()
            .replace(",", r#"\,"#);
        let mod3_path = root_path
            .join("mod, 3")
            .to_str()
            .unwrap()
            .replace(",", r#"\,"#);
        let mnt_string = format!(
            ",lowerdir+={},lowerdir+={},lowerdir+={}",
            mod1_path, mod3_path, mod2_path
        );

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
