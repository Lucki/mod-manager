use std::env;
use std::{path::PathBuf, vec};
use clap::Parser;
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

    let xdg_dirs = BaseDirectories::with_prefix("mod-manager")
        .expect("Unable to get user directories!");

    match args.action {
        Action::Activate { game, set, mut writable } => {
            let mut games_to_act_on: Vec<Game> = vec![];

            match game {
                Some(game) => {
                    games_to_act_on.push(Game::from_config(game, set).unwrap());
                },
                None => {
                    writable = false;

                    for game_config in get_game_config_list(xdg_dirs) {
                        games_to_act_on.push(match Game::from_config(game_config.file_stem().unwrap().to_str().unwrap().to_string(), None){
                            Ok(g) => g,
                            Err(error) => {
                                println!("Unable to create game object for '{:?}': {}", game_config.file_stem(), error);
                                continue;
                            }
                        });
                    };
                },
            }

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

            for game in &games_to_act_on {
                match game.deactivate() {
                    Ok(()) => (),
                    Err(error) => {
                        println!("Failed deactivating game overlay '{}': {}", game.id, error);
                    }
                }
            }
        },
        Action::Deactivate { game } => {
            let mut games_to_act_on: Vec<Game> = vec![];

            match game {
                Some(game) => {
                    games_to_act_on.push(Game::from_config(game, None).unwrap());
                },
                None => {
                    for game_config in get_game_config_list(xdg_dirs) {
                        games_to_act_on.push(match Game::from_config(game_config.file_stem().unwrap().to_str().unwrap().to_string(), None){
                            Ok(g) => g,
                            Err(error) => {
                                println!("Unable to create game object for '{:?}': {}", game_config.file_stem(), error);
                                continue;
                            }
                        });
                    }
                },
            }

            for game in games_to_act_on {
                match game.deactivate() {
                    Ok(()) => (),
                    Err(error) => {
                        println!("Failed deactivating game overlay '{}': {}", game.id, error);
                    }
                }
            }
        },
        Action::Edit { game } => {
            let mut arguments: Vec<String> = vec![];

            let editor = match env::var("EDITOR") {
                Ok(value) => value,
                Err(_) => "vi".to_owned(),
            };

            arguments.push(editor);
            arguments.push(xdg_dirs
                .place_config_file(format!("{}.toml", game))
                .expect("Unable to place config file.")
                .to_str()
                .expect("Failed converting config path to string.")
                .to_owned());

            ExternalCommand::new("editor".to_owned(), arguments, Some(true), None)
                .run()
                .unwrap();
        },
        Action::Setup { game: game_id, mod_id, path: game_path, set } => {
            let game = match game_path {
                Some(game_path) => Game::new(game_id, game_path).unwrap(),
                None => Game::from_config(game_id, set).unwrap(),
            };

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
        },
        Action::Wrap { game: game_id, command, set, writable } => {
            if command.is_empty() {
                panic!("Missing command for wrapping game");
            }

            let game = Game::from_config(game_id, set).unwrap();
            match game.wrap(ExternalCommand::new("wrap_command".to_string(), command, Some(true), None), writable) {
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
        },
    }
}

/// Return a list of all *.toml of files in a config folder.
fn get_game_config_list(xdg: BaseDirectories) -> Vec<PathBuf> {
    let mut config_files = xdg.list_config_files_once("");
    config_files
        .retain(|file| match file.extension() {
            Some(ext) => ext == "toml",
            None => false,
        });
    config_files
}
