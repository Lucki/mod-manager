# Mod-Manager

Simple game mod manager using OverlayFS.

While a mod set is activated, the manager replaces the original game file path with an OverlayFS mount which contains the original game and a set of mods.
This allows other programs to easily access modded games exactly like if they weren't modded.

Mod sets are defined in a configuration file.
Mod sets can have any number of mods and can even be nested.
An empty (`""`) active mod set name disables any mods.
The active mod set can be overridden with `--set` in the command line.

There are a two ways to handle mods, the first one is recommended:
* Start each game with:
  ~~~ text
  mod-manager wrap <game-id> -- <game-command>
  ~~~
  * More flexible - mod sets can be adjusted per command call.
  * Mods are enabled on demand.
  * Automatic updates from launchers modify the real game files.
  * Needs manual setup for every game.
  * Steam/Bottles launch options example:
    ~~~ text
    mod-manager wrap <game-id> -- %command%
    ~~~
* Run `mod-manager activate` at login and `mod-manager deactivate` at logout
  * Only needs a setup once - enable and forget solution.
  * Mods are always available.
  * Launcher with automatic updates might try to access modded folders which means:
    * If mounted immutable the update will probably fail
    * If mounted writable the update will land in a persistent cache and will take precedence over mods in the future:<br>
      `$XDG_CACHE_HOME/mod-manager/<game-id>/<set-name>_persistent`
  * Example: `systemctl --user enable mod-manager.service`

## Warning

Do **not** change the *original* files while being mounted! This is a limitation of OverlayFS and is undefined behavior.

Affected paths are:
* The original game files, moved to a folder besides the game folder with a `_mod-manager` suffix:<br>
  ~~~ text
  /path/to/game_mod-manager
  ~~~
* The used mod files:
  ~~~ text
  <mod_root_path>/<mod-name>
  ~~~
  Default: `$XDG_DATA_HOME/<game-id>/<mod-name>`

So make sure to *deactivate* mods for games before altering their *original* game or mod files.

## Usage

~~~
Simple game mod manager using OverlayFS

Usage: mod-manager <COMMAND>

Commands:
  activate    Activate a mod by mounting the OverlayFS inplace
  deactivate  Deactivate an already activated mod by unmounting the OverlayFS
  wrap        Wrap an external command in between an activation and deactivation
  setup       Setup and collect changes for a new mod by making changes to the game
  help        Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
~~~
<details><summary>Activate</summary>

~~~
Activate a mod by mounting the OverlayFS inplace

Usage: mod-manager activate [OPTIONS] [GAME]

Arguments:
  [GAME]  Identifier matching the config file

Options:
      --set <SET>  Override the "active_set" of the config file. Only applies when GAME is specified
      --writable   Mount with write access. Only applies when GAME is specified
  -h, --help       Print help
~~~
</details>
<details><summary>Deactivate</summary>

~~~
Deactivate an already activated mod by unmounting the OverlayFS

Usage: mod-manager deactivate [GAME]

Arguments:
  [GAME]  Identifier matching the config file

Options:
  -h, --help  Print help
~~~
</details>
<details><summary>Wrap</summary>

~~~
Wrap an external command in between an activation and deactivation

Usage: mod-manager wrap [OPTIONS] <GAME> -- [COMMAND]...

Arguments:
  <GAME>        Identifier matching the config file
  [COMMAND]...  Command to wrap around to

Options:
      --set <SET>  Override the "active_set" of the config file
      --writable   Mount with write access
  -h, --help       Print help
~~~
</details>
<details><summary>Setup</summary>

~~~
Setup and collect changes for a new mod by making changes to the game

Usage: mod-manager setup [OPTIONS] <GAME> <MOD>

Arguments:
  <GAME>  Identifier matching the config file. Can be a new identifier if PATH is also available.
  <MOD>   New identifier for the mod

Options:
      --path <PATH>  Creates a new config file for the game found in PATH
      --set <SET>    Override the "active_set" of the config file
  -h, --help         Print help
~~~

This directive is a bit special and needs some additional explanation. It is intended for single usage and simplifies the creation process of new configs or mods.

1. Two possibilities:
    * The config file doesn't exist yet:<br>
      The `--path="/path/to/game/files"` argument is needed. A new dummy config file will be created.
    * The config file exists already:<br>
      For this directive the only required value in the config file is the `path = "/to/the/game"`.
1. Run `mod-manager setup <game-id> <new-mod-name>`
1. Now the changes can be made to the game, e.g. dropping files or folders into the games directory structure or executing an addon installer.
1. When done press *Enter* and you'll find only the changes (basically the plain mod) in the `<mod_root_path>/<mod-name>`<br>
    Defaults to `$XDG_DATA_HOME/<game-id>/<mod-name>`
1. You can now add `<mod-name>` in your configuration file to sets.
</details>
<details><summary>Wrap</summary>

~~~
Wrap an external command in between an activation and deactivation

Usage: mod-manager wrap [OPTIONS] <GAME> -- [COMMAND]...

Arguments:
  <GAME>        Identifier matching the config file
  [COMMAND]...  Command to wrap around to

Options:
      --set <SET>  Override the "active_set" of the config file
      --writable   Mount with write access
  -h, --help       Print help
~~~
</details>

## Configuration file

Configuration files are placed in `$XDG_CONFIG_HOME/mod-manager` and written in [TOML](https://toml.io/en/latest).

See `complete.toml.example` and `minimal.toml.example` for examples.

## Installation
Make requires rust.
Build with `make build` or directly with `cargo build --release`<br>
The executable is in `target/release/mod-manager`

Install with `make install`<br>
Adjust `PREFIX` and `DESTDIR` as needed.
