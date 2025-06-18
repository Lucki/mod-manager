use std::{
    env::{current_dir, set_current_dir},
    fmt, fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
};

#[derive(PartialEq, Eq, Debug)]
pub enum MountState {
    /// Unknown mount state
    Unknown,
    /// Files are in the original path and the temporary path doesn't exist yet
    Normal,
    /// Overlay is mounted at the original path and the files are in the temporary path
    Mounted,
    /// Original path doesn't exist and the files are in the temporary path
    Moved,
    /// Known bad mount state
    Invalid,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum OverlayErrorKind {
    /// The overlay is still in use
    Used,
    /// The mount process encountered an error
    Mount,
    /// Process execution failed
    Process,
    /// The overlay is in an invalid mount state
    MountState,
    /// String conversion error
    String,
}

#[derive(Debug, Clone)]
pub struct OverlayError {
    kind: OverlayErrorKind,
    overlay: String,
    message: String,
}

impl OverlayError {
    pub fn kind(&self) -> OverlayErrorKind {
        return self.kind.clone();
    }

    pub fn overlay(&self) -> String {
        return self.overlay.clone();
    }

    pub fn message(self) -> String {
        return self.message;
    }
}

impl fmt::Display for OverlayError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let err_msg = match self.kind {
            OverlayErrorKind::Used => format!(
                "Unmounting failed, the overlay '{}' is still in use.",
                self.overlay
            ),
            OverlayErrorKind::Mount => {
                format!(
                    "The mounting helper process encountered an error: {}",
                    self.message
                )
            }
            OverlayErrorKind::Process => format!("Error running process: {}", self.message),
            OverlayErrorKind::MountState => format!(
                "'{}' is in an invalid mounting state: {}",
                self.overlay, self.message
            ),
            OverlayErrorKind::String => {
                format!("Unable to convert string: {}", self.message)
            }
        };

        write!(f, "{}", err_msg)
    }
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

    pub fn get_current_mounting_state(&self) -> Result<MountState, OverlayError> {
        // Check if the original path exists
        if !self.path.is_dir() {
            // Check if the temporary path exists
            if !self.moved_path.is_dir() {
                return Err(OverlayError {
                    kind: OverlayErrorKind::MountState,
                    overlay: self.game_id.clone(),
                    message: format!(
                        "'{}' AND '{}' both do not exist",
                        self.path.display(),
                        self.moved_path.display()
                    ),
                });
            }

            // Check if the temporary path is empty
            if is_directory_empty(&self.moved_path) {
                return Err(OverlayError {
                    kind: OverlayErrorKind::MountState,
                    overlay: self.game_id.clone(),
                    message: format!(
                        "'{}' is empty, which is unexpected",
                        self.moved_path.display()
                    ),
                });
            }

            // println!("MountState: moved");
            return Ok(MountState::Moved);
        }

        // At this point, 'path' exists
        // println!("'{}' exists", self.path.display());

        // Check if the original path is a mount
        if self.is_mounted()? {
            // Check if the temporary path exists
            if !self.moved_path.is_dir() {
                return Err(OverlayError {
                    kind: OverlayErrorKind::MountState,
                    overlay: self.game_id.clone(),
                    message: format!(
                        "'{}' is mounted, but '{}' does not exist",
                        self.path.display(),
                        self.moved_path.display()
                    ),
                });
            }

            // Check if the temporary path is empty
            if is_directory_empty(&self.moved_path) {
                return Err(OverlayError {
                    kind: OverlayErrorKind::MountState,
                    overlay: self.game_id.clone(),
                    message: format!(
                        "'{}' is mounted, but '{}' is empty",
                        self.path.display(),
                        self.moved_path.display()
                    ),
                });
            }

            // println!("MountState: mounted");
            return Ok(MountState::Mounted);
        }

        // At this point, 'path' exists and isn't mounted
        // println!("'{}' exists and is not mounted", self.path.display());

        // Check if the original path is empty
        if is_directory_empty(&self.path) {
            // Check if the temporary path exists
            if !self.moved_path.is_dir() {
                return Err(OverlayError {
                    kind: OverlayErrorKind::MountState,
                    overlay: self.game_id.clone(),
                    message: format!(
                        "'{}' is empty and '{}' does not exist",
                        self.path.display(),
                        self.moved_path.display()
                    ),
                });
            }

            // Check if the temporary path is empty
            if is_directory_empty(&self.moved_path) {
                return Err(OverlayError {
                    kind: OverlayErrorKind::MountState,
                    overlay: self.game_id.clone(),
                    message: format!(
                        "'{}' AND '{}' both are empty",
                        self.path.display(),
                        self.moved_path.display()
                    ),
                });
            }

            // Game files moved, but original path is empty and not mounted, clean up
            fs::remove_dir(&self.path).or_else(|error| {
                return Err(OverlayError {
                    kind: OverlayErrorKind::Process,
                    overlay: self.game_id.clone(),
                    message: format!(
                        "Failed removing empty directory '{}': {}",
                        self.path.display(),
                        error
                    ),
                });
            })?;

            // println!("MountState: moved");
            return Ok(MountState::Moved);
        }

        // At this point 'path' exists and is not empty
        // println!("'{}' exists and is not empty", self.path.display());

        // Check if the temporary path exists and isn't empty
        if self.moved_path.is_dir() && !is_directory_empty(&self.moved_path) {
            return Err(OverlayError {
                kind: OverlayErrorKind::MountState,
                overlay: self.game_id.clone(),
                message: format!(
                    "'{}' AND '{}' both are not empty",
                    self.path.display(),
                    self.moved_path.display()
                ),
            });
        }

        // println!("MountState: normal");
        return Ok(MountState::Normal);
    }

    pub fn clean_working_directory(&self, workdir: &PathBuf) -> Result<(), OverlayError> {
        match Command::new("pkexec")
            .arg("mod-manager-overlayfs-helper")
            .arg("cleanworkdir")
            .arg(&self.game_id)
            .arg(workdir.to_str().ok_or(OverlayError {
                kind: OverlayErrorKind::String,
                overlay: self.game_id.clone(),
                message: format!("{:?}", workdir),
            })?)
            .status()
        {
            Ok(result) => {
                if !result.success() {
                    return Err(OverlayError {
                        kind: OverlayErrorKind::Process,
                        overlay: self.game_id.clone(),
                        message: format!("'cleanworkdir' failed: {:?}", result.code()),
                    });
                }
            }
            Err(error) => {
                return Err(OverlayError {
                    kind: OverlayErrorKind::Process,
                    overlay: self.game_id.clone(),
                    message: format!("Failed executing the helper process: {}", error),
                });
            }
        }

        Ok(())
    }

    pub fn mount(&self, mount_string: String) -> Result<(), OverlayError> {
        // Make sure we're not blocking ourself by cwd == mount_point
        self.change_cwd(false)?;

        match Command::new("pkexec")
            .arg("mod-manager-overlayfs-helper")
            .arg("mount")
            .arg(&self.game_id)
            .arg(mount_string)
            .arg(self.path.to_str().ok_or(OverlayError {
                kind: OverlayErrorKind::String,
                overlay: self.game_id.clone(),
                message: format!("{:?}", self.path),
            })?)
            .status()
        {
            Ok(result) => {
                if !result.success() {
                    return Err(OverlayError {
                        kind: OverlayErrorKind::Mount,
                        overlay: self.game_id.clone(),
                        message: format!("Error mounting: {:?}", result.code()),
                    });
                }
            }
            Err(error) => {
                return Err(OverlayError {
                    kind: OverlayErrorKind::Process,
                    overlay: self.game_id.clone(),
                    message: format!("Failed executing the helper process: {}", error),
                });
            }
        }

        // Safety check if mounting was successful
        if !self.is_mounted()? {
            return Err(OverlayError {
                kind: OverlayErrorKind::MountState,
                overlay: self.game_id.clone(),
                message: String::from("Mounting wasn't sucessful"),
            });
        }

        self.change_cwd(true)?;

        Ok(())
    }

    pub fn unmount(&self) -> Result<(), OverlayError> {
        // Make sure we're not blocking ourself by cwd == mount_point
        self.change_cwd(false)?;

        // Wait some time to register we're in another cwd before trying to unmount
        thread::sleep(std::time::Duration::from_secs(1));

        // Check if programs are blocking unmount
        match Command::new("lsof")
            .arg("+f")
            .arg("--")
            .arg(&self.game_id)
            .status()
        {
            Ok(status) => {
                // lsof returns 0 if it found programs using the mountpoint
                if status.success() {
                    return Err(OverlayError {
                        kind: OverlayErrorKind::Used,
                        overlay: self.game_id.clone(),
                        message: String::from("Unmounting failed, the overlay is still in use."),
                    });
                }
            }
            Err(error) => {
                println!("Failed checking for programs using the overlay: {}", error);
                println!("Trying to continue anyway…");
            }
        }

        match Command::new("pkexec")
            .arg("mod-manager-overlayfs-helper")
            .arg("umount")
            .arg(&self.game_id)
            .status()
        {
            Ok(status) => {
                if !status.success() {
                    return Err(OverlayError {
                        kind: OverlayErrorKind::Mount,
                        overlay: self.game_id.clone(),
                        message: format!("Error unmounting: {:?}", status.code()),
                    });
                }
            }
            Err(error) => {
                return Err(OverlayError {
                    kind: OverlayErrorKind::Process,
                    overlay: self.game_id.clone(),
                    message: format!("Failed executing the helper process: {}", error),
                });
            }
        }

        // Wait some time to allow the file system to finalize
        thread::sleep(std::time::Duration::from_secs(1));

        Ok(())
    }

    fn is_mounted(&self) -> Result<bool, OverlayError> {
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
                return Err(OverlayError {
                    kind: OverlayErrorKind::Process,
                    overlay: self.game_id.clone(),
                    message: format!("Failed executing 'mountpoint' process: {}", error),
                });
            }
        }
    }

    pub fn change_cwd(&self, cwd: bool) -> Result<(), OverlayError> {
        let result = match cwd {
            true => set_current_dir(&self.cwd),
            false => set_current_dir(Path::new("/")),
        };

        result.or_else(|error| {
            Err(OverlayError {
                kind: OverlayErrorKind::Process,
                overlay: self.game_id.clone(),
                message: format!("Could not change the current working directory: {}", error),
            })
        })
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
