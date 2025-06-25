use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use crate::config::{GameConfig, ModSetConfig};
use crate::{ExternalCommand, config::CommandConfig};

#[derive(Clone)]
pub struct ModSet {
    /// Config used for this set
    config: ModSetConfig,
    /// List of nested mod sets
    mod_sets: HashMap<String, ModSet>,
}

impl ModSet {
    pub fn from_config(
        set_id: &str,
        set_config: &ModSetConfig,
        game_id: String,
        game_config: &GameConfig,
        visited: &mut HashSet<String>,
    ) -> Result<Self, String> {
        let mod_array = set_config.mods.clone();
        if mod_array.is_empty() {
            return Err(format!(
                "Array 'mods' in configuration set `{}` of game `{}` is empty",
                set_id, game_id
            ));
        };

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
                None => {}
            }
        }

        return Ok(ModSet {
            config: set_config.clone(),
            mod_sets,
        });
    }

    pub fn get_commands(
        &self,
        available_commands: &HashMap<String, CommandConfig>,
        command_list: &mut Vec<ExternalCommand>,
    ) -> () {
        match &self.config.command {
            Some(command_name) => {
                if !command_list.iter().any(|c| c.get_id() == command_name) {
                    match available_commands.get(command_name) {
                        Some(config) => match ExternalCommand::from_config(config) {
                            Ok(cmd) => command_list.push(cmd),
                            Err(err) => {
                                println!("Command creation failed for `{}`: {}", command_name, err)
                            }
                        },
                        None => {
                            println!("Command `{}` not found in config, ignoring", command_name)
                        }
                    }
                }
            }
            None => {}
        }

        for mod_set in self.mod_sets.values() {
            mod_set.get_commands(available_commands, command_list);
        }

        ()
    }

    pub fn get_mount_string(
        &self,
        root_path: PathBuf,
        mount_string: &mut String,
    ) -> Result<(), String> {
        for mod_name in &self.config.mods {
            match self.mod_sets.get(mod_name) {
                Some(set) => {
                    set.get_mount_string(root_path.clone(), mount_string)?;
                }
                None => {
                    let mod_path = root_path.join(&mod_name);
                    match mod_path.try_exists() {
                        Ok(exists) => {
                            if !exists {
                                return Err(format!(
                                    "Mod set folder `{}` does not exist",
                                    mod_path.display()
                                ));
                            }
                        }
                        Err(error) => {
                            return Err(format!(
                                "Mod set folder `{}` could not be accessed: {}",
                                mod_path.display(),
                                error
                            ));
                        }
                    }

                    let mod_path_string = escape_special_mount_chars(
                        mod_path
                            .to_str()
                            .expect("Unable to get string version of PathBuf.")
                            .to_owned(),
                    );

                    if !mount_string.contains(&mod_path_string) {
                        mount_string.push_str(&format!(",lowerdir+={}", mod_path_string));
                    }
                }
            }
        }

        Ok(())
    }

    pub fn should_run_pre_commands(&self) -> bool {
        if self.config.run_pre_command.is_some_and(|b| b == true) {
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
        if self.config.writable.is_some_and(|b| b == true) {
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
        let mut env = match &self.config.environment {
            Some(environment_config) => environment_config.variables.clone(),
            None => HashMap::new(),
        };

        for mod_name in &self.config.mods {
            match self.mod_sets.get(mod_name) {
                Some(set) => {
                    for (k, v) in set.get_environment() {
                        env.insert(k, v);
                    }
                }
                None => {}
            }
        }

        env
    }

    /// Check if the current tree includes a specific mod
    pub fn contains(&self, id: String) -> bool {
        if self.config.mods.contains(&id) {
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

        assert!(
            ModSet::from_config(
                "set1",
                &set_config,
                "test_game".to_owned(),
                &game_config,
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

        assert!(
            ModSet::from_config(
                "set1",
                &set_config,
                "test_game".to_owned(),
                &game_config,
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

        assert!(
            ModSet::from_config(
                "set1",
                &set_config,
                "test_game".to_owned(),
                &game_config,
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

        let available_commands: HashMap<String, CommandConfig> = HashMap::new();
        let mut command_list: Vec<ExternalCommand> = vec![];
        ModSet::from_config(
            "set1",
            &set_config,
            "test_game".to_owned(),
            &game_config,
            &mut HashSet::new(),
        )
        .unwrap()
        .get_commands(&available_commands, &mut command_list);
        assert!(command_list.is_empty());
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
        let mod_set = ModSet::from_config(
            "set1",
            &set_config,
            "test_game".to_owned(),
            &game_config,
            &mut HashSet::new(),
        )
        .unwrap();

        let asd_command = ExternalCommand::from_config(
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
        commands.push(&asd_command);
        commands.push(&dsa_command);

        let mut mod_set_commands: Vec<ExternalCommand> = vec![];
        mod_set.get_commands(
            &game_config.commands.unwrap().named_commands,
            &mut mod_set_commands,
        );

        for (i, command) in mod_set_commands.iter().enumerate() {
            assert_eq!(command.get_id(), commands[i].get_id());
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
        assert!(
            ModSet::from_config(
                "set1",
                &set_config,
                "test_game".to_owned(),
                &game_config,
                &mut HashSet::new(),
            )
            .unwrap()
            .get_mount_string(root_path, &mut mount_string)
            .is_ok()
        );

        assert_eq!(mount_string, mnt_string);
    }
}
