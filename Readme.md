# Mod-Manager
Simple game mod manager using OverlayFS.

This mod manager "replaces" files in-place based on pre-defined configuration rules.
Because this takes place on-demand, it's completely transparent to launchers like Steam.

## Functionality
1. The manager moves the original files out of the way and mounts them back into the original place but with any modifications layered on top.
1. The program can now be accessed and started like usual.
1. When done with the session, the overlay is unmounted again and the original files are moved back to their original place.

## Quick start tutorial
This describes the setup and startup flow for a typical folder based modification.

1. We run `mod-manager setup "My Game" "My new mod" --path="/mnt/steam/steamapps/common/awesome game"` to start the setup process for a game and it's new modification.

    Since this is the first time we set up this game we should set the location with `--path` parameter.
    The parameter is in fact optional, but already setting it here gives us tab-completion while pre-filling the value later.
    1. Because this is the first time we set up this game, an editor opens with a pre-filled template.
        The only important configuration value here is the `path = …` as that defines the location of the game to be modified.

        If you'd like a different modification save path than the default (`$XDG_DATA_HOME/<game-id>/<mod-name>`) you should also add the `mod_root_path = …` value here now.
    1. We save the configuration file and close the editor.
        The mod-manager now mounts the program files.
    1. The file explorer opens at the programs location, and we install our modification as described in their documentation.

        This is often simply copying files into specific folder and maybe overwriting files in the process.
        Sometimes this involves running an installer - do the things necessary for installation.
    1. When done, we close any programs that are currently accessing the files, like the file explorer from before.
    1. Back at the mod-manager program we press *Enter* as instructed.

        The mod-manager now unmounts the program files and moves only the modification files to the `mod_root_path`.
    1. We now separated everything needed for this modification to work in a single folder.
1. Next, we have to assign this modification to a *set*.
    We run `mod-manager edit "My Game"` which opens the editor again.
    1. `["set1"]` is already in the file from the template, let's adjust it as needed.
    1. We edit `"mod1",` inside the `["set1"]` table to be `"My new mod",`
    1. We remove the lines `"mod2",` and `"mod3",`.
    1. That's it for now, we save and close the editor.
1. This example describes modifying a Steam game, so we now head into Steam and locate the game in our list.
    1. Open the preferences for that game, either with a context click or though the gear icon.
    1. Edit the startup parameter to include the *mod-manager* together with the *set* we defined earlier:

        `mod-manager wrap "My Game" --set="set1" -- %command%`
    1. Upon closing the game preferences again we're done.
1. That's it.
    Clicking in Steam on "Launch" now starts the game with the modifications applied.

    The configuration file allows for countless customizations.
    For an exhaustive list take a look at [Configuration file](#configuration-file)

    Without going into details, you can:
    * Define a default set (`active = …`)
    * Group modifications in *sets* and nest them into other *sets*.
    * Annotate modifications with comments and links using the `#` character in front.
    * Start arbitrary different programs before launching.
    * Switch the current *set* using the `--set=…` parameter and even switching off any modifications by giving an empty parameter (`--set=""`).

## Configuration file

Configuration files are placed in `$XDG_CONFIG_HOME/mod-manager` and written in [TOML](https://toml.io/en/latest).

See `complete.toml.example` and `minimal.toml.example` for game config examples.
There's also `config.toml`, which can be used to set default search paths and adjusting the template.
See `config.example.toml` for an example.

## Installation
Make requires rust.
Build with `make build` or directly with `cargo build --release`<br>
The executable is in `target/release/mod-manager`

Install with `make install`<br>
Adjust `PREFIX` and `DESTDIR` as needed.

## Warning

Do **not** change the *original* files while being mounted! This is a limitation of OverlayFS and is undefined behavior.

Affected paths are:
* The original files, moved to a folder besides the original folder with a `_mod-manager` suffix:<br>
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
  edit        Edit or create a configuration file for a game with $EDITOR
  setup       Setup and collect changes for a new or existing mod by making changes to the game
  wrap        Wrap an external command in between an activation and deactivation
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
<details><summary>Edit</summary>

~~~
Edit or create a configuration file for a game with $EDITOR

Usage: mod-manager edit <GAME>

Arguments:
  <GAME>  Identifier matching the config file. Can be a new identifier

Options:
      --path <PATH>  Populates the "path" variable in a new config file
  -h, --help  Print help
~~~
</details>
<details><summary>Setup</summary>

~~~
Setup and collect changes for a new or existing mod by making changes to the game

Usage: mod-manager setup [OPTIONS] <GAME> <MOD>

Arguments:
  <GAME>  Identifier matching the config file. Can be a new identifier if PATH is also available
  <MOD>   New or existing identifier for the mod

Options:
      --path <PATH>  Creates a new config file for the game found in PATH
      --set <SET>    Override the "active_set" of the config file
  -h, --help         Print help
~~~

This directive is a bit special and needs some additional explanation. It simplifies the creation and editing process of new configs and mods.

1. Run `mod-manager setup <game-id> <new-mod-name>`
1. If the config file doesn't exist yet:

        The configured `$EDITOR` opens with a pre-filled template.

        Make adjustments and save the file.
        Upon closing the editor the script continues.

        The `--path="/path/to/game/files"` argument is optional and will be inserted in the template mentioned above.
1. Now the changes can be made to the game, e.g. dropping files or folders into the game directory structure or executing an add-on installer.
1. When done press *Enter*, and you'll find only the changes (basically the plain mod) in the `<mod_root_path>/<mod-name>`<br>
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
