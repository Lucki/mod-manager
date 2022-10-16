namespace ModManager {
    public class Command : Object {
        // TODO: Requires glib 2.74 - doesn't work, segfault!
        // GenericArray<string> command = new GenericArray<string>.null_terminated (0, true);
        GenericArray<string?> command = new GenericArray<string?> ();
        SubprocessLauncher launcher = new SubprocessLauncher(SubprocessFlags.NONE);
        bool wait_for_exit = true;
        internal int64 delay { get; }
        internal string id { get; }

        public Command(string id, string?[] command, bool wait_for_exit = true, int64 delay = 0) {
            _id = id;
            this.wait_for_exit = wait_for_exit;
            _delay = delay;
            // launcher.set_cwd(Environment.get_current_dir());

            foreach (var part in command) {
                this.command.add(part);
            }
            this.command.add(null);
        }

        public Command.from_toml(Toml.Table config) throws ConfigurationError {
            bool tmp_bool;
            if (!config.try_get_bool("wait_for_exit", out tmp_bool)) {
                tmp_bool = true;
            }

            int64 tmp_int;
            if (!config.try_get_int("delay", out tmp_int)) {
                tmp_int = 0;
            }

            Toml.Array? tmp_array;
            if (!config.try_get_array("command", out tmp_array)) {
                throw new ConfigurationError.KEY_MISSING("Missing \"command\" in config section for game \"$(id)\".");
            }

            unowned var array = (!) tmp_array;
            if (array.empty) {
                throw new ConfigurationError.ARRAY_EMPTY("Array \"command\" is empty in section for game \"$(id)\".");
            }

            if (array.array_type != 'v' && array.value_type != 's') {
                throw new ConfigurationError.VALUE("Array contains other values than strings.");
            }

            var tmp_command = new GenericArray<string?> ();
            for (int i = 0; i < array.size; i++) {
                string tmp_string;
                if (array.try_get_string(i, out tmp_string) && tmp_string.length > 0) {
                    tmp_command.add(tmp_string);
                }
            }
            // TODO: Remove with glib 2.74 null_terminated?
            tmp_command.add(null);

            this(config.key, tmp_command.data, tmp_bool, tmp_int);

            Toml.Table? tmp_table;
            if (config.try_get_table("environment", out tmp_table)) {
                unowned var table = (!) tmp_table;
                foreach (var key in table) {
                    string value;
                    if (table.try_get_string(key, out value)) {
                        debug("%s = %s", key, value);
                        launcher.setenv(key, value, true);
                    }
                }
            }
        }

        internal Subprocess ? run() throws Error {
            Subprocess? process;

            // null terminated argv[] !
            // TODO: Strict-Non-Null mode fails here because we need the array null terminated, wait for glib 2.74
            process = launcher.spawnv(command.data);

            if (wait_for_exit) {
                ((!) process).wait();
                process = null;
            }

            return process;
        }
    }
}
