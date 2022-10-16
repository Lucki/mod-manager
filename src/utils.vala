namespace ModManager {
    public errordomain ConfigurationError {
        KEY_MISSING,
        ARRAY_EMPTY,
        VALUE,
        FOLDER_MISSING,
        RECURSION,
    }

    public errordomain StateError {
        INVALID,
    }

    private class Utils : Object {
        internal static string program_name { get; set; }
        internal static File xdg_cache_home { get; set; }
        internal static File xdg_config_home { get; set; }
        internal static File xdg_data_home { get; set; }
        internal static File xdg_runtime { get; set; }

        internal delegate void MountFunction() throws StateError, Error;

        internal static bool change_cwd_and_run(MountFunction command) throws StateError, Error {
            var cwd = File.new_for_path(Environment.get_current_dir());

            Environment.set_current_dir("/");

            command();

            assert(cwd.get_path() != null);
            Environment.set_current_dir((!) cwd.get_path());

            return true;
        }

        internal static bool is_mountpoint(File path) {
            Subprocess process;
            try {
                process = new Subprocess(SubprocessFlags.NONE,
                                         "mountpoint",
                                         "--quiet",
                                         path.get_path());

                if (!process.wait_check()) {
                    return false;
                }
            } catch (Error e) {
                debug("\"mountpoint\" %s", e.message);
                return false;
            }

            return process.get_exit_status() == 0;
        }
    }
}
