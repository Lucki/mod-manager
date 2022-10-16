namespace ModManager {
    private class ModSet : Object {
        internal string id { get; }

        private bool _writable = false;
        internal bool writable {
            get {
                mod_sets.foreach ((key, mod_set) => {
                    _writable = _writable || mod_set.writable;
                });

                return _writable;
            }
        }

        private bool _should_run_pre_commands = false;
        internal bool should_run_pre_commands {
            get {
                mod_sets.foreach ((key, mod_set) => {
                    _should_run_pre_commands = _should_run_pre_commands || mod_set.should_run_pre_commands;
                });

                return _should_run_pre_commands;
            }
        }
        private Command? command { get; default = null; }
        // Keeping the order
        private GenericArray<string> mods { get; default = new GenericArray<string> (); }
        private HashTable<string, ModSet> mod_sets { get; default = new HashTable<string, ModSet> (str_hash, str_equal); }

        private unowned Toml.Table game_config;
        private File root_path;

        internal ModSet(Toml.Table config, Toml.Table game_config, File root_search_path, ModSet? root_mod_set = null) throws ConfigurationError {
            _id = config.key;
            root_path = root_search_path;
            this.game_config = game_config;

            debug("Current set: %s", id);

            root_mod_set = root_mod_set ?? this;

            Toml.Array? tmp_array;
            if (!config.try_get_array("mods", out tmp_array)) {
                throw new ConfigurationError.KEY_MISSING("Missing \"mods\" section for configuration for game \"$(id)\".");
            }

            if (((!) tmp_array).empty) {
                throw new ConfigurationError.ARRAY_EMPTY("Array for \"mods\" in section is empty for game \"$(id)\".");
            }

            if (((!) tmp_array).array_type != 'v' && ((!) tmp_array).value_type != 's') {
                throw new ConfigurationError.VALUE("Array contains other values than strings.");
            }

            for (int i = 0; i < ((!) tmp_array).size; i++) {
                string? tmp_string;
                if (!((!) tmp_array).try_get_string(i, out tmp_string)) {
                    warning("Unable to get key %i in set \"%s\"", i, id);
                    continue;
                }

                mods.add((!) tmp_string);

                Toml.Table? tmp_table;
                if (game_config.try_get_table((!) tmp_string, out tmp_table)) {
                    if (id in (!) root_mod_set) {
                        throw new ConfigurationError.RECURSION(@"Recursion detected in set \"$(id)\". Set \"$((!)tmp_string)\" already included.");
                    }

                    var sub_set = new ModSet((!) tmp_table, this.game_config, root_path, root_mod_set);
                    mod_sets.set((!) tmp_string, sub_set);

                    continue;
                }

                var mod_path = root_path.get_child((!) tmp_string);
                if (!mod_path.query_exists()) {
                    throw new ConfigurationError.FOLDER_MISSING(@"Folder \"$((!)mod_path.get_path())\" does not exist!");
                }
            }

            if (!config.try_get_bool("writable", out _writable)) {
                _writable = false;
            }

            if (!config.try_get_bool("run_pre_command", out _should_run_pre_commands)) {
                _should_run_pre_commands = false;
            }

            string? tmp_string;
            if (config.try_get_string("command", out tmp_string)) {
                Toml.Table? tmp_table;
                if (this.game_config.try_get_table((!) tmp_string, out tmp_table)) {
                    _command = new Command.from_toml((!) tmp_table);
                }
            }
        }

        internal void get_mount_string(ref string mount_string) {
            foreach (var mod in mods) {
                if (mod in mod_sets) {
                    var tmp_set = mod_sets.get(mod);

                    tmp_set.get_mount_string(ref mount_string);
                    continue;
                }

                var tmp_path = root_path.get_child(mod).get_path();
                if (tmp_path == null || (!) tmp_path in mount_string) {
                    debug("Mod \"%s\" already added to mount string - skipping", mod);
                    continue;
                }

                mount_string = @"$(mount_string):$((!) tmp_path)";
            }

            if (mount_string[0] == ':') {
                mount_string = mount_string[1 : mount_string.length];
            }
        }

        internal int get_commands(ref GenericArray<Command> array) {
            var added = 0;

            foreach (var mod_set in mod_sets.get_values()) {
                added += mod_set.get_commands(ref array);
            }

            if (command != null && !array.find_with_equal_func((!) command, (a, b) => {
                return a.id == b.id;
            })) {
                array.add((!) command);
                added += 1;
            }

            return added;
        }

        // Wheather this set already contains "id" itself or in one of its subsets
        internal bool contains(string id) {
            if (mods.find_with_equal_func(id, str_equal)) {
                return true;
            }

            foreach (var mod_set in mod_sets.get_keys()) {
                if (id in mod_set) {
                    return true;
                }
            }

            return false;
        }
    }
}
