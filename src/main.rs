use clap::Parser;
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::vec;
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
            let games_to_act_on: Vec<Game> = get_game_list(game, set);

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
            for game in get_game_list(game, None) {
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

            let editor = match env::var("EDITOR") {
                Ok(value) => value,
                Err(_) => "vi".to_owned(),
            };

            let config_file = get_config_file_for_id(&game_id);

            if !Path::new(&config_file).exists() {
                match File::create(&config_file) {
                    Ok(mut file) => {
                        let mut path = match PathBuf::from_str(
                            "/home/username/.local/share/Steam/steamapps/common/game",
                        ) {
                            Ok(pathbuf) => pathbuf,
                            Err(_) => panic!("Failed creating PathBuf!"),
                        };

                        match game_path {
                            Some(path_string) => path = path_string,
                            None => {}
                        }

                        let config_content = format!(
                            r#"active = ""
path = "{}"
# mod_root_path = "/mnt/mods/game"

["set1"]
mods = [
    "mod1",
    "mod2",
    "mod3",
]
"#,
                            path.to_string_lossy()
                        );

                        file.write_all(config_content.as_bytes()).unwrap();
                    }
                    Err(error) => {
                        eprintln!("Failed to create config file '{}': {}", config_file, error);
                    }
                }
            }

            arguments.push(editor);
            arguments.push(config_file);

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
            let config_file = get_config_file_for_id(&game_id);

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

            let game = Game::from_config_file(game_id, set).unwrap();

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

            let game = Game::from_config_file(game_id, set).unwrap();
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
fn get_game_list(game_id: Option<String>, override_set: Option<String>) -> Vec<Game> {
    let mut games: Vec<Game> = vec![];

    match game_id {
        Some(game) => {
            games.push(Game::from_config_file(game, override_set).unwrap());
        }
        None => {
            let config_files = get_game_config_list(get_xdg_dirs());
            create_games_from_config_files(&mut games, config_files);
        }
    }

    games
}

fn create_games_from_config_files(games_list: &mut Vec<Game>, config_files: Vec<PathBuf>) -> () {
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
    config_files.retain(|file| match file.extension() {
        Some(ext) => ext == "toml",
        None => false,
    });
    config_files
}

/// Return the config file for a game ID, possible it doesn't exist yet
fn get_config_file_for_id(game: &str) -> String {
    get_xdg_dirs()
        .place_config_file(format!("{}.toml", game))
        .expect("Unable to place config file.")
        .to_str()
        .expect("Failed converting config path to string.")
        .to_owned()
}

pub fn get_xdg_dirs() -> BaseDirectories {
    return BaseDirectories::with_prefix("mod-manager").expect("Unable to get user directories!");
}
