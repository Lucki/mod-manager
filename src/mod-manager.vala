namespace ModManager {
    class ModManager : Object {
        private enum Action {
            ACTIVATE,
            DEACTIVATE,
            WRAP,
            SETUP,
        }

        private static Action? action = null;
        private static string? game_id = null;
        private static File? game_path = null;
        private static string? game_set = null;
        private static string? mod_id = null;
        private static string[] ? command = null;
        private static bool writable = false;

        private const OptionEntry[] options = {
            // { "activate", '\0', OptionFlags.NONE, OptionArg.NONE, ref activate, "Activate a mod by mounting the OverlayFS inplace.", null },
            // { "deactivate", '\0', OptionFlags.NONE, OptionArg.NONE, ref deactivate, "Deactivate a mod by unmounting the OverlayFS inplace.", null },
            // { "wrap", '\0', OptionFlags.NONE, OptionArg.NONE, ref wrap, "Wrap an external command in between an activation and deactivation.", null },
            // { "setup", '\0', OptionFlags.NONE, OptionArg.NONE, ref setup, "Collect changes made to the game.", null },
            { null }
        };

        private const OptionEntry[] activate_options = {
            // optional
            // { "game", '\0', OptionFlags.NOALIAS, OptionArg.STRING, ref game_id, "Only mount a specific game instead of using all available config files.", "GAME" },
            { "set", '\0', OptionFlags.NOALIAS, OptionArg.STRING, ref game_set, "Override the \"active_set\" in the config file.", "SET" },
            { "writable", '\0', OptionFlags.NOALIAS, OptionArg.NONE, ref writable, "Mount with write access.", null },
            { null }
        };

        private const OptionEntry[] deactivate_options = {
            // optional
            // { "game", '\0', OptionFlags.NOALIAS, OptionArg.STRING, ref game_id, "Only unmount a specific game instead of using all available config files.", "GAME" },
            { null }
        };

        private const OptionEntry[] wrap_options = {
            // required
            // { "game", '\0', OptionFlags.NOALIAS, OptionArg.STRING, ref game_id, "Only unmount a specific game instead of using all available config files.", "GAME" },
            // { "external_command" }, // -- %command%
            // optional
            { "set", '\0', OptionFlags.NOALIAS, OptionArg.STRING, ref game_set, "Override the \"active_set\" in the config file.", "SET" },
            { "writable", '\0', OptionFlags.NOALIAS, OptionArg.NONE, ref writable, "Mount with write access.", null },
            { null }
        };

        private const OptionEntry[] setup_options = {
            // required
            // { "game", '\0', OptionFlags.NOALIAS, OptionArg.STRING, ref game_id, "Only unmount a specific game instead of using all available config files.", "GAME" },
            // { "mod", '\0', OptionFlags.NOALIAS, OptionArg.STRING, ref mod_id, "Name of the new mod.", "NAME" },
            // optional
            { "path", '\0', OptionFlags.NOALIAS, OptionArg.FILENAME, ref game_path, "Tries creating a new config file for PATH.", "PATH" },
            { "set", '\0', OptionFlags.NOALIAS, OptionArg.STRING, ref game_set, "Override the \"active_set\" in the config file.", "SET" },
            { null }
        };

        static int main(string[] args) {
            var activate_group = new OptionGroup("activate", "Activate Options:", "Activate a mod by mounting the OverlayFS inplace.");
            activate_group.add_entries(activate_options);
            var deactivate_group = new OptionGroup("deactivate", "Deactivate Options:", "Deactivate a mod by unmounting the OverlayFS inplace.");
            deactivate_group.add_entries(deactivate_options);
            var wrap_group = new OptionGroup("wrap", "Wrap Options:", "Wrap an external command in between an activation and deactivation.");
            wrap_group.add_entries(wrap_options);
            var setup_group = new OptionGroup("setup", "Setup Options:", "Collect changes made to the game.");
            setup_group.add_entries(setup_options);

            OptionContext option_context;
            option_context = new OptionContext("ACTION [ACTION-OPTION?] ARGUMENTS");
            option_context.set_summary("Simple game mod manager using OverlayFS.");
            option_context.set_description("""ACTION:
  activate                Activate a mod by mounting the OverlayFS inplace.
  deactivate              Deactivate a mod by unmounting the OverlayFS inplace.
  wrap                    Wrap an external command in between an activation and deactivation.
  setup                   Collect changes made to the game.

ARGUMENTS:                Positional
  activate:
    [GAME?]               Only target a specific game - optional.

  deactivate:
    [GAME?]               Only target a specific game - optional.

  wrap:
    GAME                  Identifier matching the config file.
    -- COMMAND ...        Command to wrap around to.

  setup:
    GAME                  Identifier matching the config file.
    MOD                   New identifier for the mod.
""");

            option_context.add_main_entries(options, null);
            option_context.add_group(activate_group);
            option_context.add_group(deactivate_group);
            option_context.add_group(wrap_group);
            option_context.add_group(setup_group);

            try {
                option_context.parse(ref args);
            } catch (OptionError e) {
                error("option parsing: %s", e.message);
            }

            Utils.program_name = "mod-manager";
            Utils.xdg_cache_home = File.new_for_path(Environment.get_user_cache_dir()).get_child(Utils.program_name);
            Utils.xdg_config_home = File.new_for_path(Environment.get_user_config_dir()).get_child(Utils.program_name);
            Utils.xdg_data_home = File.new_for_path(Environment.get_user_data_dir()).get_child(Utils.program_name);
            Utils.xdg_runtime = File.new_for_path(Environment.get_user_runtime_dir()).get_child(Utils.program_name);

            // Makes sure folders exists
            // try {
            // xdg_cache_home.make_directory_with_parents();
            // xdg_config_home.make_directory_with_parents();
            // xdg_data_home.make_directory_with_parents();
            // xdg_runtime.make_directory_with_parents();
            // } catch (Error e) {
            // printerr("%s\n", e.message);
            // return 1;
            // }

            switch (args[1]) {
            case "activate":
                action = Action.ACTIVATE;
                game_id = args[2];
                break;
            case "deactivate":
                action = Action.DEACTIVATE;
                game_id = args[2];
                break;
            case "wrap":
                action = Action.WRAP;

                if (args.length < 3) {
                    error("Missing \"GAME\" argument for \"wrap\".");
                }
                game_id = args[2];

                // "A '--' option is stripped from argv unless there are unparsed options before and after it, or some of the options after it start with '-'."
                { // WHY?
                    var i = 3;
                    if (args.length < i + 1) {
                        error("Missing \"COMMAND\" argument.");
                    }

                    if (args[i] == "--") {
                        i++;
                    }

                    if (args.length < i + 1) {
                        error("Missing \"COMMAND\" argument.");
                    }

                    command = args[i : args.length];
                }
                break;
            case "setup":
                action = Action.SETUP;

                if (args.length < 3) {
                    error("Missing \"GAME\" argument for \"setup\".");
                }
                game_id = args[2];

                if (args.length < 4) {
                    error("Missing \"MOD\" argument for \"setup\".\n\n");
                }
                mod_id = args[3];
                break;
            default:
                error("Unrecognized ACTION");
            }

            var games = new GenericArray<Game> ();
            if (game_id != null) {
                try {
                    games.add(new Game((!) game_id, game_set, game_path));
                } catch (Error e) {
                    error("Unable to process \"%s\": %s", (!) game_id, e.message);
                }
            } else {
                try {
                    string? name = null;

                    if (Utils.xdg_config_home.get_path() == null) {
                        error("Unable to open config path.");
                    }

                    var dir = Dir.open((!) Utils.xdg_config_home.get_path(), 0);

                    while ((name = dir.read_name()) != null) {
                        if (!((!) name).has_suffix(".toml")) {
                            continue;
                        }

                        try {
                            games.add(new Game(((!) name)[0 : -5]));
                        } catch (Error e) {
                            warning("Unable to process \"%s\": %s", ((!) name)[0 : -5], e.message);
                        }
                    }
                } catch (FileError e) {
                    error("%s", e.message);
                }
            }

            foreach (var game in games) {
                switch (action) {
                case Action.ACTIVATE:
                    try {
                        game.activate(writable);
                    } catch (Error e) {
                        warning("%s", e.message);
                        try_game_cleanup(game);
                    }

                    continue;
                case Action.DEACTIVATE:
                    try {
                        game.deactivate();
                    } catch (Error e) {
                        warning("%s", e.message);
                    }

                    continue;
                case Action.WRAP:
                    try {
                        game.wrap(new Command("wrap", command), writable);
                    } catch (Error e) {
                        warning("%s", e.message);
                        try_game_cleanup(game);
                    }

                    continue;
                case Action.SETUP:
                    assert(mod_id != null);

                    try {
                        game.setup((!) mod_id);
                    } catch (Error e) {
                        warning("%s", e.message);
                        try_game_cleanup(game);
                    }

                    continue;
                default:
                    continue;
                }
            }

            return 0;
        }

        private static void try_game_cleanup(Game game) {
            try {
                game.deactivate();
            } catch (Error e2) {
                warning("Cleanup failed: %s", e2.message);
            }
        }
    }
}
