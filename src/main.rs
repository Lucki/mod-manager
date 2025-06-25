use clap::Parser;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::vec;
use std::{env, fs};
use xdg::BaseDirectories;

mod config;
mod external_command;
mod game;
mod mod_set;
mod overlay;
use crate::config::{CommandConfig, MainConfig};
use crate::external_command::ExternalCommand;
use crate::game::Game;

/// Simple game mod manager using OverlayFS
#[derive(Parser)]
#[clap(version, about, verbatim_doc_comment)]
struct Cli {
    #[clap(subcommand)]
    action: Action,
}

#[derive(Parser)]
enum Action {
    /// Activate a mod by mounting the OverlayFS inplace
    #[clap(name = "activate")]
    Activate {
        /// Identifier matching the config file.
        game: Option<String>,

        /// Override the "active_set" of the config file. Only applies when GAME is specified.
        #[clap(long = "set")]
        set: Option<String>,

        /// Mount with write access. Only applies when GAME is specified.
        #[clap(long = "writable")]
        writable: bool,
    },

    /// Deactivate an already activated mod by unmounting the OverlayFS
    #[clap(name = "deactivate")]
    Deactivate {
        /// Identifier matching the config file.
        game: Option<String>,
    },

    /// Edit or create a configuration file for a game with $EDITOR
    #[clap(name = "edit")]
    Edit {
        /// Identifier matching the config file. Can be a new identifier.
        game: String,
        /// Populates the "path" setting for an empty config file.
        #[clap(long = "path")]
        path: Option<PathBuf>,
    },

    /// Setup and collect changes for a new or existing mod by making changes to the game
    #[clap(name = "setup")]
    Setup {
        /// Identifier matching the config file. Can be a new identifier if PATH is also available.
        game: String,

        /// New or existing identifier for the mod.
        #[clap(name = "MOD")]
        mod_id: String,

        /// Creates a new config file for the game found in PATH.
        #[clap(long = "path")]
        path: Option<PathBuf>,

        /// Override the "active_set" of the config file.
        #[clap(long = "set")]
        set: Option<String>,
    },

    /// Wrap an external command in between an activation and deactivation
    #[clap(name = "wrap")]
    Wrap {
        /// Identifier matching the config file.
        game: String,

        /// Command to wrap around to.
        command: Vec<String>,

        /// Override the "active_set" of the config file.
        #[clap(long = "set")]
        set: Option<String>,

        /// Mount with write access.
        #[clap(long = "writable")]
        writable: bool,
    },
}

fn main() {
    let args = Cli::parse();
    let config = read_main_config();

    match args.action {
        Action::Activate {
            game,
            set,
            mut writable,
        } => {
            if game.is_none() {
                // If no specific game is given we ignore this flag to not accidentially
                // make all mounts writable
                writable = false;
            }
            let games_to_act_on: Vec<Game> = get_game_list(game, set, &config);

            let mut failed = false;
            for game in &games_to_act_on {
                match game.activate(writable, false) {
                    Ok(()) => (),
                    Err(error) => {
                        println!("Error activating game overlay '{}': {}", game.id, error);
                        failed = true;
                        break;
                    }
                }
            }

            if !failed {
                return;
            }

            for game in games_to_act_on {
                match game.deactivate() {
                    Ok(()) => (),
                    Err(error) => {
                        println!("Failed deactivating game overlay '{}': {}", game.id, error);
                    }
                }
            }
        }
        Action::Deactivate { game } => {
            for game in get_game_list(game, None, &config) {
                match game.deactivate() {
                    Ok(()) => (),
                    Err(error) => {
                        println!("Failed deactivating game overlay '{}': {}", game.id, error);
                    }
                }
            }
        }
        Action::Edit {
            game: game_id,
            path: game_path,
        } => {
            // Try getting editor from own config first and then
            // fallback to VISUAL and EDITOR variables
            let editor = match config.editor {
                Some(editor_string) => editor_string.to_owned(),
                None => match env::var("VISUAL") {
                    Ok(value) => value,
                    Err(_) => match env::var("EDITOR") {
                        Ok(value) => value,
                        Err(_) => String::from("vi"),
                    },
                },
            };

            let game_config_file = get_config_file_path_for_id(&game_id);

            if !Path::new(&game_config_file).exists() {
                let template_config = match config.template {
                    Some(template) => template,
                    None => toml::from_str("").expect("Failed creating fallback toml."),
                };

                match fs::File::create(&game_config_file) {
                    Ok(mut file) => {
                        let path = match game_path {
                            Some(path) => path
                                .as_os_str()
                                .to_str()
                                .expect("Failed converting PathBuf to string.")
                                .to_owned(),
                            None => match template_config.path {
                                Some(value) => value,
                                None => match xdg::BaseDirectories::new() {
                                    Ok(xdg) => xdg
                                        .get_data_home()
                                        .join("Steam/steamapps/common/")
                                        .as_os_str()
                                        .to_str()
                                        .expect("Failed converting PathBuf to string.")
                                        .to_owned(),
                                    Err(err) => panic!("Failed creating default path: {}", err),
                                },
                            },
                        };

                        let mod_root = match template_config.mod_root_path {
                            Some(value) => format!("\nmod_root_path = \"{}\"", value),
                            None => String::from(""),
                        };

                        let config_content = format!(
                            r#"path = "{}"{}

["set id"]
mods = [
    "mod id",
]
"#,
                            path, mod_root
                        );

                        file.write_all(config_content.as_bytes()).unwrap();
                    }
                    Err(error) => {
                        eprintln!(
                            "Failed to create config file '{:?}': {}",
                            game_config_file, error
                        );
                    }
                }
            }

            Command::new(editor)
                .arg(
                    game_config_file
                        .as_os_str()
                        .to_str()
                        .expect("Failed converting PathBuf!")
                        .to_owned(),
                )
                .spawn()
                .unwrap()
                .wait()
                .unwrap();
        }
        Action::Setup {
            game: game_id,
            mod_id,
            path: game_path,
            set,
        } => {
            let config_file = get_config_file_path_for_id(&game_id);

            if !Path::new(&config_file).exists() {
                println!(
                    "Config file for \"{}\" doesn't exist yet, creating one…",
                    game_id
                );

                // Call own edit function and wait until done editing
                let mut edit_cmd = Command::new("mod-manager");
                edit_cmd.arg("edit").arg(game_id.clone());

                match game_path {
                    Some(path) => {
                        edit_cmd.arg(format!("--path={}", path.to_string_lossy()));
                    }
                    None => {}
                }

                edit_cmd.spawn().unwrap().wait().unwrap();
            }

            let game = Game::from_config_file(
                game_id,
                set,
                get_default_game_path_root(&config),
                get_default_mod_root(&config),
            )
            .unwrap();

            match game.setup(mod_id) {
                Ok(()) => (),
                Err(error) => {
                    println!("Failed setup game overlay '{}': {}", game.id, error);
                    match game.deactivate() {
                        Ok(()) => (),
                        Err(error) => {
                            println!("Failed deactivating game overlay '{}': {}", game.id, error);
                        }
                    }
                }
            }
        }
        Action::Wrap {
            game: game_id,
            command,
            set,
            writable,
        } => {
            if command.is_empty() {
                panic!("Missing command for wrapping game");
            }

            let game = Game::from_config_file(
                game_id,
                set,
                get_default_game_path_root(&config),
                get_default_mod_root(&config),
            )
            .unwrap();
            match game.wrap(
                ExternalCommand::from_config(&CommandConfig {
                    wait_for_exit: Some(true),
                    delay_after: None,
                    command: command,
                    environment: None,
                })
                .unwrap(),
                writable,
            ) {
                Ok(()) => (),
                Err(error) => {
                    println!("Failed wrapping game overlay '{}': {}", game.id, error);
                    match game.deactivate() {
                        Ok(()) => (),
                        Err(error) => {
                            println!("Failed deactivating game overlay '{}': {}", game.id, error);
                        }
                    }
                }
            }
        }
    }
}

/// Returns a list games, either derived from the given ID and SET or derived from all config files.
fn get_game_list(
    game_id: Option<String>,
    override_set: Option<String>,
    global_config: &MainConfig,
) -> Vec<Game> {
    let mut games: Vec<Game> = vec![];

    match game_id {
        Some(game) => {
            games.push(
                Game::from_config_file(
                    game,
                    override_set,
                    get_default_game_path_root(&global_config),
                    get_default_mod_root(&global_config),
                )
                .unwrap(),
            );
        }
        None => {
            let config_files = get_game_config_list(get_xdg_dirs());
            create_games_from_config_files(&mut games, config_files, &global_config);
        }
    }

    games
}

fn create_games_from_config_files(
    games_list: &mut Vec<Game>,
    config_files: Vec<PathBuf>,
    global_config: &MainConfig,
) -> () {
    for game_config in config_files {
        games_list.push(
            match Game::from_config_file(
                game_config
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string(),
                None,
                get_default_game_path_root(&global_config),
                get_default_mod_root(&global_config),
            ) {
                Ok(g) => g,
                Err(error) => {
                    println!(
                        "Unable to create game object for '{:?}': {}",
                        game_config.file_stem(),
                        error
                    );
                    continue;
                }
            },
        );
    }
}

/// Return a list of all *.toml of files in a config folder.
fn get_game_config_list(xdg: BaseDirectories) -> Vec<PathBuf> {
    let mut config_files = xdg.list_config_files_once("");
    config_files.retain(|file| file.extension().is_some_and(|ext| ext == "toml"));
    config_files.retain(|file| {
        file.file_name()
            .is_some_and(|basename| basename != "config.toml")
    });

    config_files
}

/// Return the config file for a game ID, possible it doesn't exist yet
fn get_config_file_path_for_id(game: &str) -> PathBuf {
    get_xdg_dirs()
        .place_config_file(format!("{}.toml", game))
        .expect("Unable to place config file.")
}

pub fn get_xdg_dirs() -> BaseDirectories {
    return BaseDirectories::with_prefix("mod-manager").expect("Unable to get user directories!");
}

fn read_main_config() -> MainConfig {
    match fs::read_to_string(get_config_file_path_for_id("config")) {
        Ok(content) => match toml::from_str(&content) {
            Ok(toml) => toml,
            Err(err) => {
                eprintln!(
                    "Error parsing 'config.toml', using default values.\nError: {}",
                    err
                );
                toml::from_str("").expect("Failed creating fallback toml.")
            }
        },
        Err(_) => toml::from_str("").expect("Failed creating fallback toml."),
    }
}

fn get_default_game_path_root(config: &MainConfig) -> Option<PathBuf> {
    match &config.default {
        Some(default_config) => default_config.game_root_path.clone(),
        None => None,
    }
}

fn get_default_mod_root(config: &MainConfig) -> Option<PathBuf> {
    match &config.default {
        Some(default_config) => default_config.mod_root_path.clone(),
        None => None,
    }
}
