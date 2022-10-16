// TomlC99 VAPI https://github.com/cktan/tomlc99
// libtoml.vapi

[CCode(cheader_filename = "toml.h")]
namespace Toml {
    [Compact]
    [CCode(cname = "toml_table_t", free_function = "", has_type_id = false)]
    public class Table {
        // Allow foreach
        public string get(int index)
        requires(index < size) {
            return (!)get_key(index);
        }

        /** Parse a string containing the full config.
         * Return a table on success, or 0 otherwise.
         * Caller must toml_free(the-return-value) after use.
         */
        [CCode(cname = "toml_parse")]
        public static Table ? from_string(string ? config, ref uint8[] error);

        /** Parse a file. Return a table on success, or 0 otherwise.
         * Caller must toml_free(the-return-value) after use.
         */
        [CCode(cname = "toml_parse_file")]
        public static Table ? from_file(GLib.FileStream ? file, ref uint8[] error);

        /** ... retrieve the key in table at keyidx. Return 0 if out of range. */
        [CCode(cname = "toml_key_in")]
        public string ? get_key(int index);

        /** ... returns 1 if key exists in tab, 0 otherwise */
        [CCode(cname = "toml_key_exists")]
        public bool contains(string key);

        /** Return the number of key-values in a table */
        private int nkval();

        /** Return the number of arrays in a table */
        private int narr();

        /** Return the number of sub-tables in a table */
        private int ntab();

        /** Return the number of key-values in a table */
        public int size {
            get {
                return nkval() + narr() + ntab();
            }
        }

        /** Return the key of a table */
        public string key {
            [CCode(cname = "toml_table_key")] get;
        }

        /**
         * Try to get a string from this {@link table} with name ''key''.
         *
         * If the retun value is true the key exists and the out parameter contains the value.
         *
         * @param key Name of the key
         * @param value Will be filled with the value or "" if the key wasn't found
         * @return Wheather the key was found.
         */
        public bool try_get_string(string key, out string value) {
            var datum = string_in(key);
            value = datum.string ?? "";

            return datum.ok;
        }

        public bool try_get_bool(string key, out bool value) {
            var datum = bool_in(key);
            value = datum.bool;

            return datum.ok;
        }

        public bool try_get_int(string key, out int64 value) {
            var datum = int_in(key);
            value = datum.int64;

            return datum.ok;
        }

        public bool try_get_double(string key, out double value) {
            var datum = double_in(key);
            value = datum.double;

            return datum.ok;
        }

        public bool try_get_timestamp(string key, out Timestamp value) {
            var datum = timestamp_in(key);
            value = datum.timestamp ?? Timestamp();

            return datum.ok;
        }

        public bool try_get_array(string key, out Array ? value) {
            value = get_array(key);
            return value != null;
        }

        public bool try_get_table(string key, out Table ? value) {
            value = get_table(key);
            return value != null;
        }

        [CCode(cname = "toml_array_in")]
        private Array ? get_array(string key);

        [CCode(cname = "toml_table_in")]
        private Table ? get_table(string key);

        /** Free the table returned by toml_parse() or toml_parse_file(). Once
         * this function is called, any handles accessed through this tab
         * directly or indirectly are no longer valid.
         */
        [DestroysInstance]
        [CCode(cname = "toml_free")]
        private void free();

        /* ... retrieve values using key. */
        [CCode(cname = "toml_string_in")]
        private Datum string_in(string key);

        [CCode(cname = "toml_bool_in")]
        private Datum bool_in(string key);

        [CCode(cname = "toml_int_in")]
        private Datum int_in(string key);

        [CCode(cname = "toml_double_in")]
        private Datum double_in(string key);

        [CCode(cname = "toml_timestamp_in")]
        private Datum timestamp_in(string key);
    }

    [Compact]
    [CCode(cname = "toml_array_t", free_function = "", has_type_id = false)]
    public class Array {
        public bool empty {
            get {
                return size < 1;
            }
        }

        public int size {
            [CCode(cname = "toml_array_nelem")] get;
        }

        public char array_type {
            [CCode(cname = "toml_array_kind")] get;
        }

        public char value_type {
            [CCode(cname = "toml_array_type")] get;
        }

        /** Return the array kind: 't'able, 'a'rray, 'v'alue, 'm'ixed */
        private char kind();

        /** For array kind 'v'alue, return the type of values
            i:int, d:double, b:bool, s:string, t:time, D:date, T:timestamp, 'm'ixed
            0 if unknown
         */
        private char? type();

        /** Return the key of an array */
        public string key {
            [CCode(cname = "toml_array_key")] get;
        }

        public bool try_get_string(int index, out string value) {
            var datum = string_at(index);
            value = datum.string ?? "";

            return datum.ok;
        }

        public bool try_get_bool(int index, out bool value) {
            var datum = bool_at(index);
            value = datum.bool;

            return datum.ok;
        }

        public bool try_get_int(int index, out int64 value) {
            var datum = int_at(index);
            value = datum.int64;

            return datum.ok;
        }

        public bool try_get_double(int index, out double value) {
            var datum = double_at(index);
            value = datum.double;

            return datum.ok;
        }

        public bool try_get_timestamp(int index, out Timestamp value) {
            var datum = timestamp_at(index);
            value = datum.timestamp ?? Timestamp();

            return datum.ok;
        }

        public bool try_get_array(int index, out Array ? value) {
            value = get_array(index);
            return value != null;
        }

        public bool try_get_table(int index, out Table ? value) {
            value = get_table(index);
            return value != null;
        }

        [CCode(cname = "toml_array_at")]
        private Array ? get_array(int index);

        [CCode(cname = "toml_table_at")]
        private Table ? get_table(int index);

        /* ... retrieve values using index. */
        [CCode(cname = "toml_string_at")]
        private Datum string_at(int index);

        [CCode(cname = "toml_bool_at")]
        private Datum bool_at(int index);

        [CCode(cname = "toml_int_at")]
        private Datum int_at(int index);

        [CCode(cname = "toml_double_at")]
        private Datum double_at(int index);

        [CCode(cname = "toml_timestamp_at")]
        private Datum timestamp_at(int index);
    }

    /** Timestamp types.
     *
     * The year, month, day, hour, minute, second, z
     * fields may be NULL if they are not relevant.
     * E.g. In a DATE
     * type, the hour, minute, second and z fields will be NULLs.
     */
    [CCode(cname = "toml_timestamp_t", has_destroy_function = false, has_type_id = false)]
    public struct Timestamp {
        [CCode(cname = "__buffer.year")]
        private int buffer_year;

        [CCode(cname = "__buffer.month")]
        private int buffer_month;

        [CCode(cname = "__buffer.day")]
        private int buffer_day;

        [CCode(cname = "__buffer.hour")]
        private int buffer_hour;

        [CCode(cname = "__buffer.minute")]
        private int buffer_minute;

        [CCode(cname = "__buffer.second")]
        private int buffer_second;

        [CCode(cname = "__buffer.millisec")]
        private int buffer_millisec;

        [CCode(cname = "__buffer.z")]
        private char buffer_z[10];

        int ? year;
        int ? month;
        int ? day;
        int ? hour;
        int ? minute;
        int ? second;
        int ? millisec;
        string ? z;
    }

    [SimpleType]
    [CCode(cname = "toml_datum_t", has_destroy_function = false, has_type_id = false)]
    public struct Datum {
        bool ok;

        [CCode(cname = "u.ts")]
        unowned Timestamp ? timestamp;

        [CCode(cname = "u.s")]
        unowned string ? string;

        [CCode(cname = "u.b")]
        bool bool;

        [CCode(cname = "u.i")]
        int64 int64;

        [CCode(cname = "u.d")]
        double double;
    }
}
