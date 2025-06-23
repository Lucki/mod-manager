use clap::Parser;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::vec;
use std::{env, fs};
use xdg::BaseDirectories;

mod external_command;
mod game;
mod mod_set;
mod overlay;
pub use crate::external_command::ExternalCommand;
pub use crate::game::Game;
pub use crate::overlay::Overlay;

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

    /// Setup and collect changes for a new mod by making changes to the game
    #[clap(name = "setup")]
    Setup {
        /// Identifier matching the config file. Can be a new identifier if PATH is also available.
        game: String,

        /// New identifier for the mod.
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
    let global_config = read_global_config();

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
            let games_to_act_on: Vec<Game> = get_game_list(game, set, &global_config);

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
            for game in get_game_list(game, None, &global_config) {
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
            let mut arguments: Vec<String> = vec![];

            let config = match fs::read_to_string(get_config_file_path_for_id("config")) {
                Ok(content) => match content.parse::<toml::Table>() {
                    Ok(toml) => toml,
                    Err(err) => {
                        eprintln!(
                            "Error parsing 'config.toml', using default values.\nError: {}",
                            err
                        );
                        toml::Table::new()
                    }
                },
                Err(_) => toml::Table::new(),
            };

            // Try getting editor from own config first and then
            // fallback to VISUAL and EDITOR variables
            let editor = match config.get("editor") {
                Some(value) => match value.as_str() {
                    Some(editor_string) => editor_string.to_owned(),
                    None => {
                        eprintln!("Failed parsing editor string, using default value.");
                        String::from("vi")
                    }
                },
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
                let template_config = match config.get("template") {
                    Some(value) => match value.as_table() {
                        Some(table) => table,
                        None => {
                            eprintln!("Failed parsing default table, using defaults!");
                            &toml::Table::new()
                        }
                    },
                    None => &toml::Table::new(),
                };

                match fs::File::create(&game_config_file) {
                    Ok(mut file) => {
                        let template_path = match xdg::BaseDirectories::new() {
                            Ok(xdg) => xdg.get_data_home().join("Steam/steamapps/common/"),
                            Err(err) => panic!("Failed creating default path: {}", err),
                        };

                        let path = match game_path {
                            Some(path) => path,
                            None => match template_config.get("path") {
                                Some(value) => match value.as_str() {
                                    Some(path) => match PathBuf::from_str(path) {
                                        Ok(pathbuf) => pathbuf,
                                        Err(err) => panic!("Failed creating PathBuf: {}", err),
                                    },
                                    None => {
                                        eprintln!(
                                            "Failed parsing default path value, using default!"
                                        );
                                        template_path
                                    }
                                },
                                None => template_path,
                            },
                        };

                        let mod_root = match template_config.get("mod_root_path") {
                            Some(value) => match value.as_str() {
                                Some(mod_path) => {
                                    format!("\nmod_root_path = \"{}\"", mod_path.to_owned())
                                }
                                None => {
                                    eprintln!("Failed parsing default mod_root_path!");
                                    String::from("")
                                }
                            },
                            None => String::from(""),
                        };

                        let config_content = format!(
                            r#"path = "{}"{}

["set id"]
mods = [
    "mod id",
]
"#,
                            path.to_string_lossy(),
                            mod_root
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

            arguments.push(editor);
            arguments.push(
                game_config_file
                    .as_os_str()
                    .to_str()
                    .expect("Failed converting PathBuf!")
                    .to_owned(),
            );

            ExternalCommand::new("editor".to_owned(), arguments, Some(true), None)
                .run()
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

                let mut arguments: Vec<String> = vec![];
                arguments.push("mod-manager".to_owned());
                arguments.push("edit".to_owned());
                arguments.push(game_id.clone());

                match game_path {
                    Some(path) => {
                        arguments.push(format!("--path={}", path.to_string_lossy()));
                    }
                    None => {}
                }

                // Call own edit function and wait until done editing
                ExternalCommand::new("edit".to_owned(), arguments, Some(true), None)
                    .run()
                    .unwrap();
            }

            let game = Game::from_config_file(
                game_id,
                set,
                get_default_game_path_root(&global_config),
                get_default_mod_root(&global_config),
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
                get_default_game_path_root(&global_config),
                get_default_mod_root(&global_config),
            )
            .unwrap();
            match game.wrap(
                ExternalCommand::new("wrap_command".to_string(), command, Some(true), None),
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
    global_config: &toml::Table,
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
    global_config: &toml::Table,
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

fn read_global_config() -> toml::Table {
    match fs::read_to_string(get_config_file_path_for_id("config")) {
        Ok(content) => match content.parse::<toml::Table>() {
            Ok(toml) => toml,
            Err(err) => {
                eprintln!(
                    "Error parsing 'config.toml', using default values.\nError: {}",
                    err
                );
                toml::Table::new()
            }
        },
        Err(_) => toml::Table::new(),
    }
}

fn get_default_game_path_root(config: &toml::Table) -> Option<PathBuf> {
    let config = match config.get("default") {
        Some(value) => match value.as_table() {
            Some(default_table) => default_table,
            None => {
                eprintln!("Config default is not a table!");
                return None;
            }
        },
        None => return None,
    };

    match config.get("game_root_path") {
        Some(value) => match value.as_str() {
            Some(path) => {
                Some(PathBuf::from_str(path).expect("Failed converting string to PathBuf!"))
            }
            None => {
                eprintln!("Failed parsing game_root_path!");
                None
            }
        },
        None => None,
    }
}

fn get_default_mod_root(config: &toml::Table) -> Option<PathBuf> {
    let config = match config.get("default") {
        Some(value) => match value.as_table() {
            Some(default_table) => default_table,
            None => {
                eprintln!("Config default is not a table!");
                return None;
            }
        },
        None => return None,
    };

    match config.get("mod_root_path") {
        Some(value) => match value.as_str() {
            Some(path) => {
                Some(PathBuf::from_str(path).expect("Failed conversting string to PathBuf!"))
            }
            None => {
                eprintln!("Failed parsing mod_root_path!");
                None
            }
        },
        None => None,
    }
}
