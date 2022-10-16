namespace ModManager {
    public class Game : Object {
        private string id;
        private int _current_state;
        private int current_state {
            get {
                try {
                    refresh_current_state();
                } catch (Error e) {
                    // TODO: be more elaborative
                    // Properties can't throw yet
                    return State.INVALID;
                }

                return _current_state;
            }
        }
        private unowned Toml.Table game_config;
        private File path;
        private File moved_path;
        private bool _writable = false;
        private bool writable {
            get {
                if (mod_tree != null) {
                    return _writable || ((!) mod_tree).writable;
                }

                return _writable;
            }
        }
        private bool _should_run_pre_commands = false;
        private bool should_run_pre_commands {
            get {
                if (mod_tree != null) {
                    return _should_run_pre_commands || ((!) mod_tree).should_run_pre_commands;
                }

                return _should_run_pre_commands;
            }
        }
        private string _mount_options = "";
        private string mount_options {
            owned get {
                if (ignore_overlays) {
                    return @"x-gvfs-hide,comment=x-gvfs-hide,lowerdir=$((!) moved_path.get_path())";
                }

                if (mod_tree != null) {
                    ((!) mod_tree).get_mount_string(ref _mount_options);
                }

                if (_mount_options == "") {
                    return @"x-gvfs-hide,comment=x-gvfs-hide,lowerdir=$((!) moved_path.get_path())";
                }

                return @"x-gvfs-hide,comment=x-gvfs-hide,lowerdir=$(_mount_options):$((!) moved_path.get_path())";
            }
        }
        private bool ignore_overlays { get; default = false; }
        private List<Subprocess> running_processes = new List<Subprocess> ();
        private string active_set;
        private ModSet? mod_tree = null;
        private string current_working_directory;

        private File game_runtime { get; }
        private File config_file { get; }
        private File game_cache { get; }
        private File game_mod_root { get; private set; }

        enum State {
            NORMAL,
            MOUNTED,
            MOVED,
            INVALID,
        }

        /**
         * Creates new config if path is given and config non existent
         */
        public Game(string id, string? set_override = null, File? game_path = null) throws ConfigurationError, StateError, FileError, Error {
            this.id = id;

            current_working_directory = Environment.get_current_dir();

            // Fixed values
            _config_file = Utils.xdg_config_home.get_child(@"$(this.id).toml");
            _game_cache = Utils.xdg_cache_home.get_child(this.id);
            _game_runtime = Utils.xdg_runtime.get_child(this.id);

            game_mod_root = Utils.xdg_data_home.get_child(this.id);

            if (game_path != null && !config_file.query_exists()) {
                info("Creating new empty configuration file \"%s\".", config_file.get_path());

                try {
                    Utils.xdg_config_home.make_directory_with_parents();
                } catch (IOError.EXISTS e) {}

                var iostream = config_file.open_readwrite();
                iostream.output_stream.write(@"path = \"$(game_path.get_path())\"".data);
                iostream.close();
            } else if (game_path != null) {
                warning("Config file \"%s\" already exists.", config_file.get_path());
            }

            parse_config();

            string tmp_string;
            if (!game_config.try_get_string("path", out tmp_string)) {
                throw new ConfigurationError.KEY_MISSING(@"\"path\" is missing for \"$(this.id)\".");
            }

            path = File.new_for_path(tmp_string);
            moved_path = File.new_for_path(@"$(tmp_string)_$(Utils.program_name)");

            if (game_config.try_get_string("mod_root_path", out tmp_string)) {
                game_mod_root = File.new_for_path(tmp_string);
            }

            if (!game_config.try_get_bool("writable", out _writable)) {
                _writable = false;
            }

            if (!game_config.try_get_bool("run_pre_commands", out _should_run_pre_commands)) {
                _should_run_pre_commands = false;
            }

            if (moved_path.get_path() == null) {
                throw new FileError.FAILED("Path null?");
            }

            if (set_override != null) {
                active_set = (!) set_override;

                if (active_set.length < 1) {
                    debug("Active set empty, ignoring overlays.");
                    _ignore_overlays = true;
                }
            } else if (!game_config.try_get_string("active", out active_set)) {
                // throw new ConfigurationError.KEY_MISSING(@"Key \"active\" is missing in configuration for \"$(this.id)\".");
                debug("Key \"active\" is missing and no override given. Ignoring overlays.");
                _ignore_overlays = true;
            }

            if (_ignore_overlays == false && active_set == "") {
                _ignore_overlays = true;
            }

            if (!ignore_overlays) {
                if (!(active_set in game_config)) {
                    throw new ConfigurationError.KEY_MISSING(@"Set \"$(active_set)\" missing in configuration for \"$(this.id)\".");
                }

                Toml.Table? tmp_table;
                if (game_config.try_get_table(active_set, out tmp_table)) {
                    mod_tree = new ModSet((!) tmp_table, game_config, game_mod_root);
                }
            }
        }

        private bool parse_config() throws FileError {
            uint8[] ? err = null;

            if (config_file.get_path() == null) {
                throw new FileError.FAILED("Path null?");
            }

            if (!config_file.query_exists()) {
                throw new FileError.EXIST("File");
            }

            var config_node = Toml.Table.from_file(FileStream.open((!) config_file.get_path(), "r"), ref err);

            if (err != null) {
                throw new FileError.FAILED(@"Error reading config file: $((string) err)");
            }

            if (config_node == null) {
                throw new FileError.FAILED(@"Error reading config file: $((string) err)");
            }

            game_config = (!) config_node;

            return true;
        }

        public void activate(bool writable = false, bool is_setup = false) throws StateError, FileError, Error {
            // Re-Mount in case the set has changed
            if (current_state == State.MOUNTED) {
                deactivate();
            }

            if (current_state == State.NORMAL) {
                path.move(moved_path, FileCopyFlags.ALL_METADATA);
            }

            if (current_state != State.MOVED) {
                throw new StateError.INVALID(@"Overlay state is not MOVED: ($(current_state))");
            }

            path.make_directory();

            var tmp_string = mount_options;

            if (writable || this.writable) {
                var persistent_name = "persistent_modless";

                // active_set should be availabe unless ignore_overlays was set on construction
                if (!ignore_overlays) {
                    persistent_name = @"$(active_set)_persistent";
                }

                if (is_setup) {
                    persistent_name = "persistent_setup";
                }

                try {
                    game_cache.make_directory_with_parents();
                } catch (IOError.EXISTS e) {}

                // The working directory (workdir) needs to be an empty directory on the same filesystem as the upper directory.
                var upperdir = game_cache.get_child(persistent_name);
                var workdir = game_cache.get_child("workdir");
                var index = workdir.get_child("index");
                var work = workdir.get_child("work");

                try {
                    upperdir.make_directory_with_parents();
                } catch (IOError.EXISTS e) {}
                try {
                    workdir.make_directory_with_parents();
                } catch (IOError.EXISTS e) {}

                // For safety the helper script will fail if one of the two doesn't exist
                // Create the missing one
                try {
                    index.make_directory_with_parents();
                } catch (IOError.EXISTS e) {}
                try {
                    work.make_directory_with_parents();
                } catch (IOError.EXISTS e) {}

                if (upperdir.get_path() == null || workdir.get_path() == null) {
                    throw new FileError.FAILED("Path null?");
                }

                var process = new Subprocess(SubprocessFlags.NONE,
                                             "pkexec",
                                             "mod-manager-overlayfs-helper",
                                             "cleanworkdir",
                                             id,
                                             workdir.get_path()
                );

                if (!process.wait_check()) {
                    throw new StateError.INVALID("Cleanworkdir failed");
                }

                tmp_string = @"$(tmp_string),upperdir=$((!) upperdir.get_path()),workdir=$((!) workdir.get_path())";
            } else if (ignore_overlays) {
                // Creating an immutable OverlayFS with a single folder.
                // OverlayFS can't mount a single folder so we're creating an empty dummy to assist us.
                var dummy = game_cache.get_child(@"$(Utils.program_name)_empty_dummy");

                try {
                    dummy.make_directory_with_parents();
                } catch (IOError.EXISTS e) {
                } catch (Error e) {
                    throw new ConfigurationError.KEY_MISSING("No active set in config or override, not writable and dummy creation failed - unable to mount.");
                }

                tmp_string = @"$(tmp_string):$((!) dummy.get_path())";
            }

            debug("Mount options: %s", tmp_string);

            // Make sure we're not blocking ourseld by cwd == mount_point
            Utils.change_cwd_and_run(() => {
                var process = new Subprocess(SubprocessFlags.STDERR_MERGE,
                                             "pkexec",
                                             "mod-manager-overlayfs-helper",
                                             "mount",
                                             id,
                                             tmp_string,
                                             path.get_path()
                );

                if (!process.wait_check()) {
                    throw new StateError.INVALID("Mounting failed");
                }
            });

            assert(Utils.is_mountpoint(path));

            // Reset CWD in case we moved it previously
            if (Environment.set_current_dir(current_working_directory) != 0) {
                debug("Failed setting cwd - programs might be starting in the wrong working directory.");
            }

            var tmp_array = new GenericArray<Command> ();
            if (should_run_pre_commands || (mod_tree != null && ((!) mod_tree).get_commands(ref tmp_array) > 0)) {
                try {
                    run_pre_run_commands();
                } catch (Error e) {
                    warning("Unable to start (some) commands.");
                    warning("%s", e.message);
                }
            }
        }

        public void deactivate() throws StateError, FileError, Error {
            if (game_runtime.query_exists() && game_runtime.get_path() != null) {
                List<string> pids = new List<string> ();
                string? name = null;

                try {
                    var dir = Dir.open((!) game_runtime.get_path(), 0);
                    while ((name = dir.read_name()) != null) {
                        pids.append((!) name);
                    }
                } catch (FileError e) {
                    warning("%s", e.message);
                }

                foreach (var tmp_pid in pids) {
                    // FIXME: int in UNIX only
                    Pid pid = int.parse(tmp_pid);
                    Posix.kill(pid, ProcessSignal.TERM);
                    // TODO: Timeout to SIGKILL?
                    // FIXME: Processes ignoring SIGTERM might block unmount later

                    var file = game_runtime.get_child(tmp_pid);
                    try {
                        file.delete ();
                    } catch (Error e) {
                        warning("%s", e.message);
                        continue;
                    }
                }
            }

            switch (current_state) {
            case State.NORMAL:
                return;
            case State.MOUNTED:
                // Make sure we're not blocking ourseld by cwd == mount_point
                Utils.change_cwd_and_run(() => {
                    var process = new Subprocess(SubprocessFlags.NONE,
                                                 "pkexec",
                                                 "mod-manager-overlayfs-helper",
                                                 "umount",
                                                 id,
                                                 null);

                    if (!process.wait_check()) {
                        throw new StateError.INVALID("Unmounting failed");
                    }

                    // Wait some time to allow the file system to finalize
                    Posix.sleep(2);
                });
                break;
            default:
                break;
            }

            if (current_state == State.MOVED) {
                try {
                    path.delete ();
                } catch (IOError.NOT_FOUND e) {}

                moved_path.move(path, FileCopyFlags.ALL_METADATA);
            }
        }

        public void wrap(Command command, bool writable = false) throws FileError, StateError, Error {
            activate(writable);

            try {
                command.run();

                // Allow programs to finalize so they don't occupy the mounting point
                debug("Allowing programs to finalize for 2s.");
                Posix.sleep(2);
            } catch (Error e) {
                warning("Wrapped process: %s", e.message);
            }

            deactivate();
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
        internal void setup(string mod_id) throws ConfigurationError, StateError, FileError, Error {
            var new_mod_path = game_mod_root.get_child(mod_id);
            assert(new_mod_path.get_path() != null);

            if (new_mod_path.query_exists()) {
                debug("Mod \"%s\" already exists.\n", mod_id);
                throw new ConfigurationError.VALUE(@"Folder \"$((!)new_mod_path.get_path())\" already exists.");
            }

            activate(true, true);

            print("Make the required changes to the game folder.\nE.g. installing an addon or placing mod files into the folder structure.\nPress Enter when done setting up.\n");
            stdin.read_line();

            deactivate();

            var persistent_dir = game_cache.get_child("persistent_setup");
            persistent_dir.move(new_mod_path, FileCopyFlags.ALL_METADATA);

            print("Your changes are in \"%s\".\n", (!) new_mod_path.get_path());
        }

        private void run_pre_run_commands() throws Error {
            var cmd_list = new GenericArray<Command> ();

            try {
                game_runtime.make_directory_with_parents();
            } catch (IOError.EXISTS e) {}

            if (should_run_pre_commands) {
                Toml.Array? tmp_array;

                if (game_config.try_get_array("pre_command", out tmp_array) && ((!) tmp_array).array_type == 't') {
                    unowned var array = (!) tmp_array;

                    for (int i = 0; i < array.size; i++) {
                        Toml.Table? tmp_table;

                        if (!array.try_get_table(i, out tmp_table)) {
                            continue;
                        }

                        try {
                            cmd_list.add(new Command.from_toml((!) tmp_table));
                        } catch (Error e) {
                            warning("%s", e.message);
                            continue;
                        }
                    }
                }
            }

            if (mod_tree != null) {
                ((!) mod_tree).get_commands(ref cmd_list);
            }

            foreach (var command in cmd_list) {
                try {
                    Subprocess? process;
                    if ((process = command.run()) != null) {
                        running_processes.append((!) process);
                    }
                } catch (Error e) {
                    warning("%s", e.message);
                    continue;
                }

                // Wait here to be in the same thread?
                if (command.delay > 0) {
                    Posix.sleep((uint) command.delay);
                }
            }

            foreach (var process in running_processes) {
                var pid = process.get_identifier();
                if (pid == null) {
                    debug("Process already closed.");
                    continue;
                }

                var file = game_runtime.get_child((!) pid);

                try {
                    file.create(GLib.FileCreateFlags.REPLACE_DESTINATION | FileCreateFlags.PRIVATE);
                } catch (Error e) {
                    warning("%s", e.message);
                    continue;
                }
            }
        }

        private void refresh_current_state() throws StateError, FileError, Error {
            if (path.get_path() == null || moved_path.get_path() == null) {
                throw new FileError.FAILED("Path null?");
            }

            // Check if original path exists
            if (!path.query_exists()) {
                if (!moved_path.query_exists()) {
                    _current_state = State.INVALID;
                    throw new StateError.INVALID(@"$(id) is in an invalid state.\nPath and moved path don't exist.");
                }
                var dir = Dir.open((!) moved_path.get_path(), 0);
                if (dir.read_name() == null) {
                    _current_state = State.INVALID;
                    throw new StateError.INVALID(@"$(id) is in an invalid state.\nPath doesn't exist and moved path is empty.");
                }

                _current_state = State.MOVED;
                return;
            }

            // Check if original path is a mount
            if (Utils.is_mountpoint(path)) {
                if (!moved_path.query_exists()) {
                    _current_state = State.INVALID;
                    throw new StateError.INVALID(@"$(id) is in an invalid state.\nPath is mounted but moved path doesn't exist.");
                }

                var dir = Dir.open((!) moved_path.get_path(), 0);
                if (dir.read_name() == null) {
                    _current_state = State.INVALID;
                    throw new StateError.INVALID(@"$(id) is in an invalid state.\nPath is mounted but moved path is empty.");
                }

                _current_state = State.MOUNTED;
                return;
            }

            // Check if original path is empty
            var dir = Dir.open((!) path.get_path(), 0);
            if (dir.read_name() == null) {
                if (!moved_path.query_exists()) {
                    _current_state = State.INVALID;
                    throw new StateError.INVALID(@"$(id) is in an invalid state.\nPath is empty and move path doesn't exist.");
                }

                var dir_temporary = Dir.open((!) moved_path.get_path(), 0);
                if (dir_temporary.read_name() == null) {
                    _current_state = State.INVALID;
                    throw new StateError.INVALID(@"$(id) is in an invalid state.\nPath and moved path are empty.");
                }

                // Game is moved but original path is empty and not mounted, clean up
                path.delete ();
                _current_state = State.MOVED;
                return;
            }

            // Check if temporary path also exists and isn't empty
            if (moved_path.query_exists()) {
                var dir_temporary = Dir.open((!) moved_path.get_path(), 0);
                if (dir_temporary.read_name() != null) {
                    _current_state = State.INVALID;
                    throw new StateError.INVALID(@"$(id) is in an invalid state.\nPath and moved path aren't empty.");
                }
            }

            _current_state = State.NORMAL;
            return;
        }
    }
}
