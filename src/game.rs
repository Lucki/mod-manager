use rustix::process::{Signal, kill_process};
use std::collections::HashSet;
use std::io::{ErrorKind, stdin};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::{fs, str::FromStr, vec};
use toml::{Value, map::Map};
use xdg::BaseDirectories;

use crate::ExternalCommand;
use crate::get_xdg_dirs;
use crate::mod_set::ModSet;
use crate::overlay::MountState;
use crate::overlay::Overlay;

pub struct Game {
    pub id: String,
    config: Map<String, Value>,
    overlay: Overlay,
    writable: bool,
    should_run_pre_commands: bool,
    mount_options: String,
    /// Activate set with Some(set), deactivate overlays with None
    active_set: Option<String>,
    /// Can be None when overlays are ignored
    mod_tree: Option<ModSet>,

    path: PathBuf,
    moved_path: PathBuf,
    xdg_dirs: BaseDirectories,
    mod_root_path: PathBuf,
}

impl Game {
    pub fn new(id: String, game_path: PathBuf) -> Result<Self, String> {
        let xdg_dirs_config = get_xdg_dirs();

        let config_file = xdg_dirs_config
            .place_config_file(format!("{}.toml", id))
            .or_else(|error| return Err(format!("Failed to create game config file: {}", error)))?;

        // Check if '$XDG_CONFIG_HOME/mod-manager/$id.toml', fail if it does
        match config_file.try_exists() {
            Ok(exists) => {
                if exists {
                    return Err(format!("Config '{}' already exists", config_file.display()));
                }
            }
            Err(error) => {
                return Err(format!("{}", error));
            }
        }

        // Write toml entry 'path = $path' to '$XDG_CONFIG_HOME/mod-manager/$id.toml'
        std::fs::write(
            config_file,
            format!(
                r#"path = "{}""#,
                game_path
                    .to_str()
                    .ok_or(format!("Failed to parse path {:?}", game_path))?
            ),
        )
        .or_else(|error| return Err(format!("Could not write config file: {}", error)))?;

        // Return game generated from toml config
        return Game::from_config_file(id, None);
    }

    pub fn from_config_file(id: String, set_override: Option<String>) -> Result<Self, String> {
        let xdg_dirs_config = get_xdg_dirs();

        let config_file = xdg_dirs_config
            .find_config_file(format!("{}.toml", id))
            .ok_or(format!("No config file found for game '{}'", id))?;

        let config = fs::read_to_string(&config_file)
            .or_else(|error| {
                return Err(format!(
                    "Could not read config for game '{}': {}",
                    id, error
                ));
            })?
            .parse::<Value>()
            .or_else(|error| {
                return Err(format!(
                    "Could not parse config for game '{}': {}",
                    id, error
                ));
            })?
            .as_table()
            .ok_or(format!(
                "Could not parse config for game '{}': Root is not a table",
                id
            ))?
            .to_owned();

        return Game::from_config(id, set_override, config);
    }

    fn from_config(
        id: String,
        set_override: Option<String>,
        config: toml::Table,
    ) -> Result<Self, String> {
        let xdg_dirs = Game::get_xdg_dirs(id.clone())?;

        // 'path' field is required, fail if it doesn't exist
        let path = match config.get("path") {
            Some(path) => PathBuf::from_str(
                path.as_str()
                    .ok_or(format!(
                        "Expected string for field 'path' for game '{}'",
                        id
                    ))?
                    .trim_end_matches('/'),
            )
            .or_else(|error| Err(format!("Could not get 'path' for game '{}': {}", id, error)))?,
            None => return Err(format!("Could not get 'path' for game '{}'", id)),
        };

        let moved_path = PathBuf::from_str(&format!(
            "{}_mod-manager",
            path.to_str()
                .ok_or(format!("Failed to parse path {:?}", path))?
        ))
        .or_else(|error| {
            Err(format!(
                "Could not create moved_path for game '{}': {}",
                id, error
            ))
        })?;

        let mod_root_path = match config.get("mod_root_path") {
            Some(path) => {
                let path = PathBuf::from_str(path.as_str().ok_or(format!(
                    "Expected string for field 'mod_root_path' for game '{}'",
                    id
                ))?)
                .or_else(|error| {
                    Err(format!(
                        "Could not get 'mod_root_path' for game '{}': {}",
                        id, error
                    ))
                })?
                .canonicalize()
                .or_else(|error| {
                    Err(format!(
                        "Unable to get absolute mod root path for game '{}': {}",
                        id, error
                    ))
                })?;
                // TODO: Re-evaluate this check
                if !path.exists() {
                    return Err(format!(
                        "'mod_root_path' is not an existing directory for game '{}': {}",
                        id,
                        path.display()
                    ));
                }
                path
            }
            None => xdg_dirs.get_data_file(&id),
        };

        let writable = match config.get("writable") {
            Some(value) => value.as_bool().ok_or(format!(
                "Expected boolean for field 'writable' in game '{}'",
                id
            ))?,
            None => false,
        };

        let should_run_pre_commands = match config.get("run_pre_commands") {
            Some(value) => value.as_bool().ok_or(format!(
                "Expected boolean for field 'run_pre_commands' in game '{}'",
                id
            ))?,
            None => false,
        };

        let active_set = match set_override {
            Some(set) => match set.is_empty() {
                true => None,
                false => Some(set),
            },
            None => match config.get("active") {
                Some(value) => {
                    let s = value
                        .as_str()
                        .ok_or(format!(
                            "Expected string for field 'active' in game '{}'",
                            id
                        ))?
                        .to_owned();
                    match s.is_empty() {
                        true => None,
                        false => Some(s),
                    }
                }
                None => None,
            },
        };

        // let mut tmp_vec = Vec::<String>::new();
        let mod_tree = match &active_set {
            Some(a_set) => match config.get(a_set) {
                Some(value) => Some(ModSet::from_config(
                    &a_set,
                    value.as_table().ok_or(format!(
                        "Expected table for field '{}' in game '{}'",
                        &a_set, id
                    ))?,
                    id.clone(),
                    &config,
                    mod_root_path.clone(),
                    &mut HashSet::new(),
                )?),
                None => return Err(format!("Field '{}' in game '{}' not found", &a_set, id)),
            },
            None => None,
        };

        let mount_options = match &mod_tree {
            Some(tree) => {
                let mut s = "".to_owned();
                tree.get_mount_string(&mut s);
                let m = moved_path
                    .to_str()
                    .ok_or(format!(
                        "Failed to convert '{}' to a string",
                        moved_path.display()
                    ))?
                    .replace(":", r#"\:"#);

                if s == "" {
                    format!("x-gvfs-hide,comment=x-gvfs-hide,lowerdir={}", m)
                } else {
                    format!("x-gvfs-hide,comment=x-gvfs-hide,lowerdir={}:{}", s, m)
                }
            }
            None => format!(
                "x-gvfs-hide,comment=x-gvfs-hide,lowerdir={}",
                moved_path
                    .to_str()
                    .ok_or(format!(
                        "Failed to convert '{}' to a string",
                        moved_path.display()
                    ))?
                    .replace(":", r#"\:"#)
            ),
        };

        let overlay = Overlay::new(id.clone(), path.clone(), moved_path.clone());

        return Ok(Game {
            id,
            mod_root_path,
            xdg_dirs,
            config,
            path,
            moved_path,
            writable,
            should_run_pre_commands,
            active_set,
            mod_tree,
            overlay,
            mount_options,
        });
    }

    pub fn activate(&self, writable: bool, is_setup: bool) -> Result<(), String> {
        // Re-mount in case the mod set has changed
        if self.overlay.get_current_mounting_state()? == MountState::MOUNTED {
            self.deactivate()?;
        }

        if self.overlay.get_current_mounting_state()? == MountState::NORMAL {
            // Move path to mounted_path
            fs::rename(&self.path, &self.moved_path).or_else(|e| {
                Err(format!(
                    "Moving game dir for game '{}' failed: {}",
                    &self.id, e
                ))
            })?;
        }

        // Check if we're good to go, panic if not
        if self.overlay.get_current_mounting_state()? != MountState::MOVED {
            return Err(format!(
                "Game '{}' is in an unexpected mounting state, aborting",
                &self.id
            ));
        }

        // Create 'path' directory
        fs::create_dir_all(&self.path).or_else(|error| {
            return Err(format!(
                "Failed creating game '{}' directory '{}': {}",
                self.id,
                self.path.display(),
                error
            ));
        })?;

        let mount_string = self.get_mount_string(writable, is_setup)?;
        match self.overlay.mount(mount_string) {
            Ok(_) => {}
            Err(error) => return Err(format!("Unable to mount game '{}': {}", self.id, error)),
        }

        let pre_commands = match &self.mod_tree {
            Some(tree) => Some(tree.get_commands()),
            None => None,
        };

        if self.should_run_pre_commands()
            || (pre_commands.is_some() && !pre_commands.as_ref().unwrap().is_empty())
        {
            self.run_pre_commands(pre_commands);
        }

        return Ok(());
    }

    pub fn deactivate(&self) -> Result<(), String> {
        let runtime_files = self.xdg_dirs.list_runtime_files("");

        for runtime_file in runtime_files {
            let pid = match runtime_file.file_stem() {
                Some(pid) => match pid.to_str() {
                    Some(pid) => match pid.parse::<i32>() {
                        Ok(pid) => pid,
                        Err(error) => {
                            println!(
                                "Failed parsing pid '{}' to i32 for game '{}': {}",
                                pid, self.id, error
                            );
                            continue;
                        }
                    },
                    None => {
                        println!(
                            "Failed converting OsStr '{:?}' to String for game '{}'",
                            pid, self.id
                        );
                        continue;
                    }
                },
                None => {
                    println!(
                        "Failed getting basename of file '{}' for game '{}'",
                        runtime_file.display(),
                        self.id
                    );
                    continue;
                }
            };

            // kill pid
            match kill_process(
                rustix::process::Pid::from_raw(pid).expect("Failed creating PID object"),
                Signal::Term,
            ) {
                Ok(_) => (),
                Err(error) => {
                    println!(
                        "Terminating existing process '{}' for game '{}' failed: {}.",
                        pid, self.id, error
                    );
                    continue;
                }
            }

            match fs::remove_file(&runtime_file) {
                Ok(_) => {}
                Err(error) => println!(
                    "Unable to remove PID file '{}' for game '{}': {}",
                    runtime_file.display(),
                    self.id,
                    error
                ),
            }
        }

        match self.overlay.get_current_mounting_state()? {
            MountState::NORMAL => {
                return Ok(());
            }
            MountState::MOUNTED => match self.overlay.unmount() {
                Ok(_) => {}
                Err(error) => {
                    return Err(format!("Unable to unmount game '{}': {}", self.id, error));
                }
            },
            MountState::UNKNOWN => {
                return Err(format!(
                    "Unable to retrieve the current mounting state for game '{}'.",
                    self.id
                ));
            }
            MountState::INVALID => {
                return Err(format!("Game '{}' is in an invalid mount state.", self.id));
            }
            MountState::MOVED => {}
        }

        if self.overlay.get_current_mounting_state()? == MountState::MOVED {
            match fs::remove_dir(&self.path) {
                Ok(_) => (),
                Err(e) => match e.kind() {
                    ErrorKind::NotFound => (),
                    _ => {
                        return Err(format!(
                            "Unable to remove the empty game '{}' directory '{}': {}.",
                            self.id,
                            self.path.display(),
                            e
                        ));
                    }
                },
            }

            fs::rename(&self.moved_path, &self.path).or_else(|error| {
                Err(format!(
                    "Unable to move the game '{}' back to it's original location: {}",
                    self.id, error
                ))
            })?;
        }

        self.overlay.change_cwd(true).or_else(|error| {
            return Err(format!(
                "Could not change the current working directory for '{}' to '{}': {}",
                self.id,
                self.path.display(),
                error
            ));
        })?;

        Ok(())
    }

    pub fn wrap(&self, command: ExternalCommand, writable: bool) -> Result<(), String> {
        self.activate(writable, false)?;

        match command.run() {
            Ok(_) => (),
            Err(error) => {
                println!(
                    "Unable to execute command for game '{}': {}",
                    self.id, error
                );
            }
        }

        self.deactivate()?;

        Ok(())
    }

    /**
     * Convenience function to set up a new mod.
     *
     * It's basically an automated {@link activate} and {@link deactivate} with time to do modifications between.
     * Afterwards the isolated modifications will be moved to the mod root path with the name ''mod_id''.
     *
     * This happens on top of an active set if configured or overridden.
     *
     * @param mod_id Name of the new mod. The changes will end up in path "mod_root_path/''mod_id''".
     */
    pub fn setup(&self, new_mod_id: String) -> Result<(), String> {
        let new_mod_path = self.mod_root_path.join(&new_mod_id);
        let cache_path = self
            .xdg_dirs
            .create_cache_directory("persistent_setup")
            .or_else(|error| {
                return Err(format!(
                    "Failed creating 'persistent_setup' for game '{}': {}",
                    self.id, error
                ));
            })?;

        if new_mod_path.is_dir() {
            return Err(format!(
                "Mod '{}' already exists at '{}'",
                new_mod_id,
                new_mod_path.display()
            ));
        }

        self.activate(true, true)?;

        let mut line = String::new();
        println!(
            "Make the required changes to the game folder: '{}'\nE.g. installing an addon or placing mod files into the folder structure.\nPress Enter here when done setting up.\n",
            self.path.display()
        );

        match stdin().read_line(&mut line) {
            Ok(_) => (),
            Err(error) => println!("Reading of stdin failed: {}", error),
        }

        self.deactivate()?;

        match copy_dir_all(&cache_path, &new_mod_path) {
            Ok(_) => {
                println!("Folder copied successfully");

                match fs::remove_dir_all(&cache_path) {
                    Ok(_) => println!("Temporary folder removed successfully"),
                    Err(e) => println!("Error removing temporary folder: {}", e),
                }

                println!(
                    "Your mod files are in '{}'. To apply the mod, add '{}' into a mod set for '{}'.",
                    new_mod_path.display(),
                    new_mod_id,
                    self.id
                );
            }
            Err(e) => {
                println!("Error copying folder: {}", e);
                println!(
                    "Your changes are still in the temporary folder, please handle them manually: {:?}",
                    &cache_path
                );
            }
        }

        Ok(())
    }

    fn get_mount_string(&self, writable: bool, is_setup: bool) -> Result<String, String> {
        let mut mount_string = self.mount_options.clone();
        if writable
            || self.writable
            || self
                .mod_tree
                .as_ref()
                .is_some_and(|t| t.should_be_writable())
        {
            let mut persistent_name = "persistent_modless".to_string();

            if self.active_set.is_some() && self.mod_tree.is_some() {
                persistent_name = format!("{}_persistent", self.active_set.as_ref().unwrap());
            }

            if is_setup {
                persistent_name = "persistent_setup".to_string();
            }

            // TODO: Evaluate if it's worth moving the upperdir into and from $XDG_DATA_HOME

            // The working directory (workdir) needs to be an empty directory on the same filesystem as the upper directory
            let upperdir = self
                .xdg_dirs
                .create_cache_directory(persistent_name)
                .or_else(|error| {
                    return Err(format!(
                        "Failed creating 'upperdir' for game '{}': {}",
                        self.id, error
                    ));
                })?;
            let workdir = self
                .xdg_dirs
                .create_cache_directory("workdir")
                .or_else(|error| {
                    return Err(format!(
                        "Failed creating 'workdir' for game '{}': {}",
                        self.id, error
                    ));
                })?;
            self.xdg_dirs
                .create_cache_directory("workdir/index")
                .or_else(|error| {
                    return Err(format!(
                        "Failed creating 'workdir/index' for game '{}': {}",
                        self.id, error
                    ));
                })?;
            self.xdg_dirs
                .create_cache_directory("workdir/work")
                .or_else(|error| {
                    return Err(format!(
                        "Failed creating 'workdir/work' for game '{}': {}",
                        self.id, error
                    ));
                })?;

            match self.overlay.clean_working_directory(&workdir) {
                Ok(_) => {}
                Err(error) => {
                    return Err(format!(
                        "Unable to clean the workdir for game '{}': {}",
                        self.id, error
                    ));
                }
            }

            // Mods can change but we will get ESTALE for some configurations
            // Force: index=off,metacopy=off
            // https://bbs.archlinux.org/viewtopic.php?pid=2031633#p2031633
            mount_string = format!(
                "{},upperdir={},workdir={},index=off,metacopy=off",
                mount_string,
                upperdir.to_str().ok_or(format!(
                    "Failed converting 'upperdir' string: {}",
                    upperdir.display()
                ))?,
                workdir.to_str().ok_or(format!(
                    "Failed converting 'workdir' string: {}",
                    workdir.display()
                ))?
            );
        } else if self.active_set.is_none() && self.mod_tree.is_none() {
            // Creating an immutable OverlayFS with a single folder.
            // OverlayFS can't mount a single folder so we're creating an empty dummy to assist us.
            let dummy = self
                .xdg_dirs
                .create_cache_directory("mod-manager_empty_dummy")
                .or_else(|error| {
                    return Err(format!(
                        "Failed creating game '{}' cache directory: {}",
                        self.id, error
                    ));
                })?;

            mount_string = format!(
                "{}:{}",
                mount_string,
                dummy
                    .to_str()
                    .ok_or(format!(
                        "Failed converting string '{}' for game '{}' cache directory",
                        dummy.display(),
                        self.id
                    ))?
                    .replace(":", r#"\:"#)
            );
        }

        return Ok(mount_string);
    }

    fn should_run_pre_commands(&self) -> bool {
        if self.should_run_pre_commands {
            return true;
        }

        if self.mod_tree.is_some() {
            return self.mod_tree.as_ref().unwrap().should_run_pre_commands();
        }

        return false;
    }

    fn run_pre_commands(&self, mod_tree_commands: Option<Vec<&ExternalCommand>>) {
        let mut commands: Vec<ExternalCommand> = vec![];

        match self.xdg_dirs.create_runtime_directory("") {
            Ok(_) => (),
            Err(error) => {
                println!(
                    "Could not create runtime directory for game '{}': {}\nNo pre commands were started.",
                    self.id, error
                );
                return;
            }
        }

        if self.should_run_pre_commands() {
            match self.config.get("pre_command") {
                Some(value) => match value.as_array() {
                    Some(array) => {
                        for (i, pre_command_table) in array.iter().enumerate() {
                            let pre_command_table = match pre_command_table.as_table() {
                                Some(table) => table,
                                None => {
                                    println!(
                                        "Could not get 'pre_command' table for game '{}'",
                                        self.id
                                    );
                                    continue;
                                }
                            };

                            let pre_command = match ExternalCommand::from_config(
                                self.id.clone(),
                                i.to_string(),
                                pre_command_table,
                            ) {
                                Ok(c) => c,
                                Err(error) => {
                                    println!(
                                        "Failed creating pre command '{}' for game '{}': {}",
                                        i.to_string(),
                                        error,
                                        self.id
                                    );
                                    continue;
                                }
                            };

                            commands.push(pre_command);
                        }
                    }
                    None => println!("'pre_command' must be an array for game '{}'.", self.id),
                },
                None => println!(
                    "'pre_command' expected to be defined for game '{}' but wasn't.",
                    self.id
                ),
            }
        }

        if mod_tree_commands.is_some() {
            for command in mod_tree_commands.unwrap() {
                commands.push(command.to_owned());
            }
        }

        let mut running_processes: Vec<Child> = vec![];
        for command in commands {
            let process = match command.run() {
                Ok(p) => p,
                Err(error) => {
                    println!(
                        "Failed to run pre-command for game '{}': {}",
                        self.id, error
                    );
                    None
                }
            };

            if process.is_some() {
                running_processes.push(process.unwrap());
            }
        }

        for process in running_processes {
            let pid = process.id();
            let pid_file = match self.xdg_dirs.place_runtime_file(format!("{}", pid)) {
                Ok(path) => path,
                Err(error) => {
                    println!(
                        "Failed to get PID of process '{}' for game '{}': {}\nThe process won't be terminated when deactivating.",
                        pid, self.id, error
                    );
                    continue;
                }
            };

            match std::fs::write(pid_file, "") {
                Ok(()) => {}
                Err(error) => {
                    println!(
                        "Could not write PID file for process '{}' for game '{}': {}\nThe process won't be terminated when deactivating.",
                        pid, self.id, error
                    );
                    continue;
                }
            }
        }
    }

    fn get_xdg_dirs(id: String) -> Result<BaseDirectories, String> {
        return BaseDirectories::with_prefix(format!("mod-manager/{}", id))
            .or_else(|error| return Err(format!("Couldn't get user directories: {}", error)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_empty_config() {
        let id = String::from("test: game");
        let xdg_dirs = BaseDirectories::with_prefix(format!("mod-manager")).unwrap();
        let game_path = PathBuf::from(String::from("test/game"));
        let config_name = format!("{}.toml", id);

        // clean file if it already exists for an even testing ground
        match xdg_dirs.find_config_file(config_name.clone()) {
            Some(path) => fs::remove_file(path).unwrap(),
            None => (),
        }

        let game = Game::new(id.clone(), game_path);
        assert!(game.is_ok());
        assert_eq!(game.unwrap().path.to_str().unwrap(), "test/game");

        // clean remnants after test
        match xdg_dirs.find_config_file(config_name.clone()) {
            Some(path) => fs::remove_file(path).unwrap(),
            None => assert!(false, "Config file doesn't exist."),
        }
    }

    #[test]
    fn mount_path_default_set() {
        let config = get_test_config();
        let game = Game::from_config("test: game".to_string(), None, config).unwrap();
        let mount_string = game.get_mount_string(false, false).unwrap();

        let game_path = PathBuf::from(String::from("test/game: asd"))
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace(":", r#"\:"#);
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
        let mnt_string = format!(
            "x-gvfs-hide,comment=x-gvfs-hide,lowerdir={}:{}:{}:{}_mod-manager",
            mod1_path, mod2_path, mod3_path, game_path
        );

        assert_eq!(mnt_string, mount_string);
    }

    #[test]
    fn mount_path_set_override() {
        let config = get_test_config();
        let game =
            Game::from_config("test: game".to_string(), Some("set2".to_string()), config).unwrap();
        let mount_string = game.get_mount_string(false, false).unwrap();

        let game_path = PathBuf::from(String::from("test/game: asd"))
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace(":", r#"\:"#);
        let root_path = PathBuf::from(String::from("test/mod: root"))
            .canonicalize()
            .unwrap();
        let mod1_path = root_path
            .join("mod 1")
            .to_str()
            .unwrap()
            .replace(":", r#"\:"#);
        let mnt_string = format!(
            "x-gvfs-hide,comment=x-gvfs-hide,lowerdir={}:{}_mod-manager",
            mod1_path, game_path
        );

        assert_eq!(mnt_string, mount_string);
    }

    #[test]
    fn mount_path_empty_set_override() {
        let config = get_test_config();
        let game =
            Game::from_config("test: game".to_string(), Some("".to_string()), config).unwrap();
        let mount_string = game.get_mount_string(false, false).unwrap();

        let game_path = PathBuf::from(String::from("test/game: asd"))
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace(":", r#"\:"#);
        let cache_path = game
            .xdg_dirs
            .get_cache_home()
            .join("mod-manager_empty_dummy")
            .to_str()
            .unwrap()
            .replace(":", r#"\:"#);
        let mnt_string = format!(
            "x-gvfs-hide,comment=x-gvfs-hide,lowerdir={}_mod-manager:{}",
            game_path, cache_path
        );

        assert_eq!(mnt_string, mount_string);
    }

    fn get_test_config() -> toml::map::Map<String, toml::Value> {
        let config_file = PathBuf::from("./test/test.toml");
        let game_path = PathBuf::from("./test/game: asd");
        let mod_root = PathBuf::from("./test/mod: root");
        let mut config = fs::read_to_string(&config_file)
            .unwrap()
            .parse::<Value>()
            .unwrap()
            .as_table()
            .unwrap()
            .to_owned();

        config.insert(
            "path".to_string(),
            toml::Value::try_from(fs::canonicalize(&game_path).unwrap().to_str().unwrap()).unwrap(),
        );
        config.insert(
            "mod_root_path".to_string(),
            toml::Value::try_from(fs::canonicalize(&mod_root).unwrap().to_str().unwrap()).unwrap(),
        );

        return config;
    }
}

// https://stackoverflow.com/a/65192210
fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}
