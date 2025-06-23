use rustix::process::{Signal, kill_process};
use std::collections::HashSet;
use std::fmt;
use std::io::{ErrorKind, stdin};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::{fs, str::FromStr, vec};
use toml::{Value, map::Map};
use xdg::BaseDirectories;

use crate::ExternalCommand;
use crate::get_xdg_dirs;
use crate::mod_set::ModSet;
use crate::overlay::Overlay;
use crate::overlay::{MountState, OverlayErrorKind};

#[derive(Debug, Clone)]
pub struct GameError {
    kind: String,
    id: String,
    message: String,
}

impl fmt::Display for GameError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "GameError {{ kind: {}, overlay: {}, message: {} }}",
            self.kind, self.id, self.message
        )
    }
}

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
    pub fn from_config_file(
        id: String,
        set_override: Option<String>,
        default_path_root: Option<PathBuf>,
        default_mod_root: Option<PathBuf>,
    ) -> Result<Self, String> {
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

        return Game::from_config(
            id,
            set_override,
            config,
            default_path_root,
            default_mod_root,
        );
    }

    fn from_config(
        id: String,
        set_override: Option<String>,
        config: toml::Table,
        default_path_root: Option<PathBuf>,
        default_mod_root: Option<PathBuf>,
    ) -> Result<Self, String> {
        let xdg_dirs = Game::get_xdg_dirs(id.clone())?;

        let mut path = match config.get("path") {
            Some(value) => match value.as_str() {
                Some(path) => {
                    PathBuf::from_str(path).expect("Failed converting string to PathBuf!")
                }
                None => {
                    eprintln!("Failed parsing path string, using default.");
                    PathBuf::default()
                }
            },
            None => PathBuf::default(),
        };
        if path == PathBuf::default() {
            path = match default_path_root {
                Some(root_path) => root_path.join(&id),
                None => {
                    return Err(format!(
                        "Config for '{}' is missing path value and no default 'game_root_path' found.",
                        id
                    ));
                }
            };
        }

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

        let mut mod_root_path = match config.get("mod_root_path") {
            Some(value) => match value.as_str() {
                Some(path) => {
                    PathBuf::from_str(path).expect("Failed converting string to PathBuf!")
                }
                None => {
                    eprintln!("Failed parsing mod_root_path string, using default.");
                    PathBuf::default()
                }
            },
            None => PathBuf::default(),
        };
        if mod_root_path == PathBuf::default() {
            mod_root_path = match default_mod_root {
                Some(root_path) => root_path.join(&id),
                None => xdg_dirs.get_data_home(),
            };
        }

        if !mod_root_path.exists() {
            std::fs::create_dir_all(&mod_root_path).unwrap();
        }

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

        let mut mount_options = "x-gvfs-hide,comment=x-gvfs-hide".to_owned();
        match &mod_tree {
            Some(tree) => {
                let mut s = "".to_owned();
                tree.get_mount_string(&mut s);
                let m = escape_special_mount_chars(
                    moved_path
                        .to_str()
                        .ok_or(format!(
                            "Failed to convert '{}' to a string",
                            moved_path.display()
                        ))?
                        .to_owned(),
                );

                if !s.is_empty() {
                    mount_options.push_str(&s);
                }
                mount_options.push_str(&format!(",lowerdir+={}", m));
            }
            None => {
                mount_options.push_str(&format!(
                    ",lowerdir+={}",
                    escape_special_mount_chars(
                        moved_path
                            .to_str()
                            .ok_or(format!(
                                "Failed to convert '{}' to a string",
                                moved_path.display()
                            ))?
                            .to_owned()
                    )
                ));
            }
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
        if self.overlay.get_current_mounting_state().or_else(|e| {
            Err(format!(
                "Failed validating the mounting state for '{}': {}",
                e.overlay(),
                e.message()
            ))
        })? == MountState::Mounted
        {
            self.deactivate()
                .or_else(|e| Err(format!("Error deactivating overlay: {}", e)))?;
        }

        if self.overlay.get_current_mounting_state().or_else(|e| {
            Err(format!(
                "Failed validating the mounting state for '{}': {}",
                e.overlay(),
                e.message()
            ))
        })? == MountState::Normal
        {
            // Move path to mounted_path
            fs::rename(&self.path, &self.moved_path).or_else(|e| {
                Err(format!(
                    "Moving game dir for game '{}' failed: {}",
                    &self.id, e
                ))
            })?;
        }

        // Check if we're good to go, panic if not
        if self.overlay.get_current_mounting_state().or_else(|e| {
            Err(format!(
                "Failed validating the mounting state for '{}': {}",
                e.overlay(),
                e.message()
            ))
        })? != MountState::Moved
        {
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

    pub fn deactivate(&self) -> Result<(), GameError> {
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

        match self.overlay.get_current_mounting_state().or_else(|e| {
            Err(GameError {
                kind: String::from("overlay"),
                id: self.id.clone(),
                message: format!("Error getting current mount state: {}", e),
            })
        })? {
            MountState::Normal => {
                return Ok(());
            }
            MountState::Mounted => match self.overlay.unmount() {
                Ok(_) => {}
                Err(error) => {
                    if error.kind() == OverlayErrorKind::Used {
                        return Err(GameError {
                            kind: String::from("Overlay in use."),
                            id: self.id.clone(),
                            message: error.message(),
                        });
                    }
                    return Err(GameError {
                        kind: String::from("overlay"),
                        id: self.id.clone(),
                        message: error.message(),
                    });
                }
            },
            MountState::Unknown => {
                return Err(GameError {
                    kind: String::from("overlay"),
                    id: self.id.clone(),
                    message: String::from(
                        "Unable to retrieve the current mounting state for the game.",
                    ),
                });
            }
            MountState::Invalid => {
                return Err(GameError {
                    kind: String::from("overlay"),
                    id: self.id.clone(),
                    message: String::from("The game is in an invalid mounting state."),
                });
            }
            MountState::Moved => {}
        }

        if self.overlay.get_current_mounting_state().or_else(|e| {
            Err(GameError {
                kind: String::from("overlay"),
                id: self.id.clone(),
                message: format!("Error getting current mount state: {}", e),
            })
        })? == MountState::Moved
        {
            match fs::remove_dir(&self.path) {
                Ok(_) => (),
                Err(e) => match e.kind() {
                    ErrorKind::NotFound => (),
                    _ => {
                        return Err(GameError {
                            kind: String::from("fs"),
                            id: self.id.clone(),
                            message: format!(
                                "Unable to remove the empty game directory '{}': {}.",
                                self.path.display(),
                                e
                            ),
                        });
                    }
                },
            }

            fs::rename(&self.moved_path, &self.path).or_else(|error| {
                Err(GameError {
                    kind: String::from("fs"),
                    id: self.id.clone(),
                    message: format!(
                        "Unable to move game files back to it's original location: {}",
                        error
                    ),
                })
            })?;
        }

        self.overlay.change_cwd(true).or_else(|error| {
            Err(GameError {
                kind: String::from("process"),
                id: self.id.clone(),
                message: format!(
                    "Unable to change current working directory to {}: {}",
                    self.path.display(),
                    error
                ),
            })
        })?;

        Ok(())
    }

    pub fn wrap(&self, mut command: ExternalCommand, writable: bool) -> Result<(), String> {
        self.activate(writable, false)?;

        match &self.mod_tree {
            Some(tree) => command.add_environment_variables(&mut tree.get_environment()),
            None => {}
        }

        match command.run() {
            Ok(_) => (),
            Err(error) => {
                println!(
                    "Unable to execute command for game '{}': {}",
                    self.id, error
                );
            }
        }

        self.deactivate()
            .or_else(|e| Err(format!("Error deactivating overlay: {}", e)))?;

        Ok(())
    }

    ///
    /// Convenience function to set up a new mod.
    ///
    /// It's basically an automated {@link activate} and {@link deactivate} with time to do modifications between.
    /// Afterwards the isolated modifications will be moved to the mod root path with the name ''mod_id''.
    ///
    /// This happens on top of an active set if configured or overridden.
    ///
    /// @param mod_id Name of the new mod. The changes will end up in path "mod_root_path/''mod_id''".
    ///
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

        match open::that(self.path.as_os_str()) {
            Ok(_) => (),
            Err(_) => (),
        }

        match stdin().read_line(&mut line) {
            Ok(_) => (),
            Err(error) => println!("Reading of stdin failed: {}", error),
        }

        while match self.deactivate() {
            Ok(_) => false,
            Err(e) => {
                if e.kind == String::from("Overlay in use.") {
                    true
                } else {
                    return Err(format!("{}", e));
                }
            }
        } {
            println!(
                "The overlay is currently in use. Please close the listed programs and press Enter again."
            );

            match stdin().read_line(&mut line) {
                Ok(_) => (),
                Err(error) => println!("Reading of stdin failed: {}", error),
            }
        }

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
                "{},lowerdir+={}",
                mount_string,
                escape_special_mount_chars(
                    dummy
                        .to_str()
                        .ok_or(format!(
                            "Failed converting string '{}' for game '{}' cache directory",
                            dummy.display(),
                            self.id
                        ))?
                        .to_owned()
                )
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

fn escape_special_mount_chars(string: String) -> String {
    string.replace(",", r#"\,"#)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_path_default_set() {
        let config = get_test_config();
        let game = Game::from_config("test, game".to_string(), None, config, None, None).unwrap();
        let mount_string = game.get_mount_string(false, false).unwrap();

        let game_path = PathBuf::from(String::from("test/game, asd"))
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace(",", r#"\,"#);
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
            "x-gvfs-hide,comment=x-gvfs-hide,lowerdir+={},lowerdir+={},lowerdir+={},lowerdir+={}_mod-manager",
            mod1_path, mod2_path, mod3_path, game_path
        );

        assert_eq!(mnt_string, mount_string);
    }

    #[test]
    fn mount_path_set_override() {
        let config = get_test_config();
        let game = Game::from_config(
            "test, game".to_string(),
            Some("set2".to_string()),
            config,
            None,
            None,
        )
        .unwrap();
        let mount_string = game.get_mount_string(false, false).unwrap();

        let game_path = PathBuf::from(String::from("test/game, asd"))
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace(",", r#"\,"#);
        let root_path = PathBuf::from(String::from("test/mod, root"))
            .canonicalize()
            .unwrap();
        let mod1_path = root_path
            .join("mod 1")
            .to_str()
            .unwrap()
            .replace(",", r#"\,"#);
        let mnt_string = format!(
            "x-gvfs-hide,comment=x-gvfs-hide,lowerdir+={},lowerdir+={}_mod-manager",
            mod1_path, game_path
        );

        assert_eq!(mnt_string, mount_string);
    }

    #[test]
    fn mount_path_empty_set_override() {
        let config = get_test_config();
        let game = Game::from_config(
            "test, game".to_string(),
            Some("".to_string()),
            config,
            None,
            None,
        )
        .unwrap();
        let mount_string = game.get_mount_string(false, false).unwrap();

        let game_path = PathBuf::from(String::from("test/game, asd"))
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace(",", r#"\,"#);
        let cache_path = game
            .xdg_dirs
            .get_cache_home()
            .join("mod-manager_empty_dummy")
            .to_str()
            .unwrap()
            .replace(",", r#"\,"#);
        let mnt_string = format!(
            "x-gvfs-hide,comment=x-gvfs-hide,lowerdir+={}_mod-manager,lowerdir+={}",
            game_path, cache_path
        );

        assert_eq!(mnt_string, mount_string);
    }

    fn get_test_config() -> toml::map::Map<String, toml::Value> {
        let config_file = PathBuf::from("./test/test.toml");
        let game_path = PathBuf::from("./test/game, asd");
        let mod_root = PathBuf::from("./test/mod, root");
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
