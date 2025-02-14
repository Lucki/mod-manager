use std::{
    env::{current_dir, set_current_dir},
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
};

#[derive(PartialEq, Eq, Debug)]
pub enum MountState {
    /// Unknown mount state
    UNKNOWN,
    /// Files are in the original path and the temporary path doesn't exist yet
    NORMAL,
    /// Overlay is mounted at the original path and the files are in the temporary path
    MOUNTED,
    /// Original path doesn't exist and the files are in the temporary path
    MOVED,
    /// Known bad mount state
    INVALID,
}

pub struct Overlay {
    game_id: String,
    path: PathBuf,
    moved_path: PathBuf,
    cwd: PathBuf,
}

impl Overlay {
    pub fn new(id: String, path: PathBuf, moved_path: PathBuf) -> Overlay {
        Overlay {
            game_id: id,
            path,
            moved_path,
            cwd: current_dir().unwrap(),
        }
    }

    pub fn get_current_mounting_state(&self) -> Result<MountState, String> {
        // Check if the original path exists
        if !self.path.is_dir() {
            // Check if the temporary path exists
            if !self.moved_path.is_dir() {
                return Err(format!(
                    "'{}' is in an invalid overlay mount state: '{}' AND '{}' do not exist",
                    self.game_id,
                    self.path.display(),
                    self.moved_path.display()
                ));
            }

            // Check if the temporary path is empty
            if is_directory_empty(&self.moved_path) {
                return Err(format!(
                    "'{}' is in an invalid overlay mount state: '{}' is empty, which is unexpected",
                    self.game_id,
                    self.moved_path.display()
                ));
            }

            // println!("MountState: moved");
            return Ok(MountState::MOVED);
        }

        // At this point, 'path' exists
        // println!("'{}' exists", self.path.display());

        // Check if the original path is a mount
        if self.is_mounted()? {
            // Check if the temporary path exists
            if !self.moved_path.is_dir() {
                return Err(format!("'{}' is in an invalid overlay mount state: '{}' is mounted, but '{}' does not exist", self.game_id, self.path.display(), self.moved_path.display()));
            }

            // Check if the temporary path is empty
            if is_directory_empty(&self.moved_path) {
                return Err(format!(
                    "'{}' is in an invalid overlay mount state: '{}' is mounted, but '{}' is empty",
                    self.game_id,
                    self.path.display(),
                    self.moved_path.display()
                ));
            }

            // println!("MountState: mounted");
            return Ok(MountState::MOUNTED);
        }

        // At this point, 'path' exists and isn't mounted
        // println!("'{}' exists and is not mounted", self.path.display());

        // Check if the original path is empty
        if is_directory_empty(&self.path) {
            // Check if the temporary path exists
            if !self.moved_path.is_dir() {
                return Err(format!("'{}' is in an invalid overlay mount state: '{}' is empty and '{}' does not exist", self.game_id, self.path.display(), self.moved_path.display()));
            }

            // Check if the temporary path is empty
            if is_directory_empty(&self.moved_path) {
                return Err(format!(
                    "'{}' is in an invalid overlay mount state: '{}' AND '{}' are empty",
                    self.game_id,
                    self.path.display(),
                    self.moved_path.display()
                ));
            }

            // Game files moved, but original path is empty and not mounted, clean up
            fs::remove_dir(&self.path).or_else(|error| {
                return Err(format!(
                    "Failed removing empty directory '{}': {}",
                    self.game_id, error
                ));
            })?;

            // println!("MountState: moved");
            return Ok(MountState::MOVED);
        }

        // At this point 'path' exists and is not empty
        // println!("'{}' exists and is not empty", self.path.display());

        // Check if the temporary path exists and isn't empty
        if self.moved_path.is_dir() && !is_directory_empty(&self.moved_path) {
            return Err(format!(
                "'{}' is in an invalid overlay mount state: '{}' AND '{}' are not empty",
                self.game_id,
                self.path.display(),
                self.moved_path.display()
            ));
        }

        // println!("MountState: normal");
        return Ok(MountState::NORMAL);
    }

    pub fn clean_working_directory(&self, workdir: &PathBuf) -> Result<(), String> {
        match Command::new("pkexec")
            .arg("mod-manager-overlayfs-helper")
            .arg("cleanworkdir")
            .arg(&self.game_id)
            .arg(workdir.to_str().ok_or(format!(
                "Failed converting 'workdir' string: {}",
                workdir.display()
            ))?)
            .status()
        {
            Ok(result) => {
                if !result.success() {
                    return Err(format!(
                        "'cleanworkdir' failed: {}",
                        result
                            .code()
                            .ok_or(format!(
                                "Failed converting status code to string for game '{}'",
                                self.game_id
                            ))?
                            .to_string()
                    ));
                }
            }
            Err(error) => {
                return Err(format!("Failed calling 'cleanworkdir': {}", error));
            }
        }

        Ok(())
    }

    pub fn mount(&self, mount_string: String) -> Result<(), String> {
        // Make sure we're not blocking ourself by cwd == mount_point
        self.change_cwd(false).or_else(|error| {
            return Err(format!(
                "Could not change the current working directory for game '{}': {}",
                self.game_id, error
            ));
        })?;

        match Command::new("pkexec")
            .arg("mod-manager-overlayfs-helper")
            .arg("mount")
            .arg(&self.game_id)
            .arg(mount_string)
            .arg(
                self.path
                    .to_str()
                    .ok_or(format!("Failed converting string: {}", self.path.display()))?,
            )
            .status()
        {
            Ok(result) => {
                if !result.success() {
                    return Err(format!(
                        "'mount' failed: {}",
                        result
                            .code()
                            .ok_or(format!(
                                "Failed converting status code to string for game '{}'",
                                self.game_id
                            ))?
                            .to_string()
                    ));
                }
            }
            Err(error) => {
                return Err(format!("Failed calling 'mount': {}", error));
            }
        }

        // Safety check if mounting was successful
        if !self.is_mounted()? {
            return Err(format!("Mounting '{}' wasn't successful ", self.game_id));
        }

        self.change_cwd(true).or_else(|error| {
            return Err(format!(
                "Could not change the current working directory for game '{}' to '{}': {}",
                self.game_id,
                self.path.display(),
                error
            ));
        })?;

        Ok(())
    }

    pub fn unmount(&self) -> Result<(), String> {
        // Make sure we're not blocking ourself by cwd == mount_point
        self.change_cwd(false).or_else(|error| {
            return Err(format!(
                "Could not change the current working directory for game '{}': {}",
                self.game_id, error
            ));
        })?;

        // Wait some time to register we're in another cwd before trying to unmount
        thread::sleep(std::time::Duration::from_secs(1));

        match Command::new("pkexec")
            .arg("mod-manager-overlayfs-helper")
            .arg("umount")
            .arg(&self.game_id)
            .status()
        {
            Ok(status) => {
                if !status.success() {
                    return Err(format!(
                        "Mod manager overlayfs helper umount failed: {}",
                        status
                            .code()
                            .ok_or(format!(
                                "Failed converting status code to string for game '{}'",
                                self.game_id
                            ))?
                            .to_string()
                    ));
                }
            }
            Err(error) => {
                return Err(format!("Unable to run umount process: {}", error));
            }
        }

        // Wait some time to allow the file system to finalize
        thread::sleep(std::time::Duration::from_secs(1));

        Ok(())
    }

    fn is_mounted(&self) -> Result<bool, String> {
        match Command::new("mountpoint")
            .arg("--quiet")
            .arg(&self.path)
            .status()
        {
            Ok(status) => {
                if !status.success() {
                    return Ok(false);
                } else {
                    return Ok(true);
                }
            }
            Err(error) => {
                return Err(format!("Unable to run 'mountpoint' process: {}", error));
            }
        }
    }

    pub fn change_cwd(&self, cwd: bool) -> Result<(), std::io::Error> {
        match cwd {
            true => return set_current_dir(&self.cwd),
            false => return set_current_dir(Path::new("/")),
        }
    }
}

fn is_directory_empty(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }

    let mut empty = true;
    for entry in fs::read_dir(path).unwrap() {
        match entry {
            Ok(_dir_entry) => {
                empty = false;
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return false,
        }
    }

    empty
}
