# Mod-Manager

Simple game mod manager using OverlayFS.

While activated it replaces the original game file path with an OverlayFS mount which contains the original game and a set of mods.
This allows other programs to easily access modded games exactly like unmodded.

Mod sets can be defined in the configuration file and sets are temporarily changeable with `--set`.
Sets can have any number of mods and can even be nested.

There are a two ways to handle mods:
* Run `mod-manager activate` on login and `mod-manager deactivate` on logout
  * Easy and only have to be setup once - enable and forget solution.
  * Mods are always available.
  * Launcher with automatic updates might try to access modded folders which means:
    * If mounted immutable the update will probably fail
    * If mounted writable the update will land in a persistent cache and will take precedence over mods in the future.
      `$XDG_CACHE_HOME/mod-manager/<game-id>/persistent`
  * Example: `systemctl --user enable mod-manager.service`
* Start the game with `mod-manager wrap <game-id> -- <game-command>`
  * More flexible - sets can be adjusted per command call.
  * Mods are enabled on demand.
  * Automatic updates from launchers modify the real game files.
  * Needs manual setup for every game.
  * Steam/Bottles example: `mod-manager wrap <game-id> -- %command%`

## Usage

~~~
usage: mod-manager [-h] {activate,deactivate,wrap,setup} ...

Simple game mod manager using OverlayFS

positional arguments:
  {activate,deactivate,wrap,setup}
                        Possible actions
    activate            Activate a mod by mounting the OverlayFS inplace
    deactivate          Deactivate an already activated mod by unmounting the OverlayFS
    wrap                Wrap an external command in between an activation and deactivation
    setup               Setup and collect changes for a new mod by making changes to the game

options:
  -h, --help            show this help message and exit
~~~
<details><summary>Activate</summary>

~~~
usage: mod-manager activate [-h] [--set [SET]] [--writable] [game]

positional arguments:
  game         ID that matches the configuration file, if None all config files will be affected

options:
  -h, --help   show this help message and exit
  --set [SET]  The mod set to activate, overwrites the activated set in the config file
  --writable   Ensure the merged directories are writable. Written changes can be found in the cache folder.
~~~
</details>
<details><summary>Deactivate</summary>

~~~
usage: mod-manager deactivate [-h] [game]

positional arguments:
  game        ID that matches the configuration file, if None all config files will be affected

options:
  -h, --help  show this help message and exit
~~~
</details>
<details><summary>Wrap</summary>

~~~
usage: mod-manager wrap [-h] [--set [SET]] [--writable] game -- external_command ...

positional arguments:
  game              ID that matches the configuration file, if None all config files will be affected
  external_command  Command to wrap around to. Placed last after POSIX style ' -- '

options:
  -h, --help        show this help message and exit
  --set [SET]       The mod set to activate, overwrites the activated set in the config file
  --writable        Ensure the merged directories are writable. Written changes can be found in the cache folder.
~~~
</details>
<details><summary>Setup</summary>

~~~
usage: mod-manager setup [-h] game mod

positional arguments:
  game        ID that matches the configuration file, if None all config files will be affected
  mod         The name of the new mod

options:
  -h, --help  show this help message and exit
~~~

This directive is a bit special and needs some additional explanation. It is intended for single usage and simplifies the creation process of new mods.

1. The config file has to exist already but for this directive the only required value in the config file is the `path = "/to/the/game"`.
1. Run `mod-manager setup <game-id> <new-mod-name>`
1. Now the changes can be made to the game, e.g. dropping files or folders into the games directory structure or executing an addon installer.
1. When done press *Enter* and you'll find only the changes (basically the plain mod) in the `<mod_root_path>/<mod-name>`

    Defaults to `$XDG_DATA_HOME/<game-id>/<mod-name>`
1. You can now add `<mod-name>` in your configuration file to sets.
</details>

## Configuration file

Configuration files are placed in `$XDG_CONFIG_HOME/mod-manager` and written in [TOML](https://toml.io/en/latest).

See `complete.toml.example` and `minimal.toml.example` for examples.

## Installation

This manager requires `python`, `python-pyxdg` and `python-tomlkit`.

Install with `make install`. Adjust the variables `DESTDIR` and `PREFIX` if needed.
