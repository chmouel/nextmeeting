//! PID file management for the server.
//!
//! This module provides functionality to create and manage a PID file
//! to prevent multiple instances of the server from running.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process;

use tracing::{debug, info, warn};

use crate::error::{ServerError, ServerResult};

/// PID file manager.
///
/// Creates a PID file on creation and removes it on drop.
pub struct PidFile {
    path: PathBuf,
}

impl PidFile {
    /// Creates a new PID file at the specified path.
    ///
    /// Returns an error if another instance is already running.
    pub fn create(path: impl Into<PathBuf>) -> ServerResult<Self> {
        let path = path.into();

        // Check if PID file already exists
        if path.exists() {
            match Self::read_pid(&path) {
                Ok(pid) => {
                    if Self::is_process_running(pid) {
                        return Err(ServerError::already_running(path.to_string_lossy()));
                    }
                    // Stale PID file, remove it
                    warn!(
                        path = %path.display(),
                        pid = pid,
                        "Removing stale PID file"
                    );
                    fs::remove_file(&path)?;
                }
                Err(_) => {
                    // Invalid PID file, remove it
                    warn!(
                        path = %path.display(),
                        "Removing invalid PID file"
                    );
                    fs::remove_file(&path)?;
                }
            }
        }

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        // Write our PID
        let pid = process::id();
        let mut file = File::create(&path)?;
        writeln!(file, "{}", pid)?;
        file.sync_all()?;

        info!(path = %path.display(), pid = pid, "Created PID file");

        Ok(Self { path })
    }

    /// Returns the path to the PID file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the current process ID.
    pub fn pid(&self) -> u32 {
        process::id()
    }

    /// Reads the PID from a file.
    fn read_pid(path: &Path) -> ServerResult<u32> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let pid = contents.trim().parse::<u32>().map_err(|_| {
            ServerError::config(format!("Invalid PID in file: {}", contents.trim()))
        })?;
        Ok(pid)
    }

    /// Checks if a process with the given PID is running.
    #[cfg(unix)]
    fn is_process_running(pid: u32) -> bool {
        // On Unix, send signal 0 to check if process exists
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }

    /// Checks if a process with the given PID is running (non-Unix).
    #[cfg(not(unix))]
    fn is_process_running(_pid: u32) -> bool {
        // On non-Unix, we can't reliably check, so assume it's running
        true
    }
}

impl Drop for PidFile {
    fn drop(&mut self) {
        if self.path.exists() {
            if let Err(e) = fs::remove_file(&self.path) {
                warn!(
                    path = %self.path.display(),
                    error = %e,
                    "Failed to remove PID file"
                );
            } else {
                debug!(path = %self.path.display(), "Removed PID file");
            }
        }
    }
}

/// Returns the default PID file path.
///
/// Uses `$XDG_RUNTIME_DIR/nextmeeting.pid` if available,
/// otherwise falls back to `/tmp/nextmeeting-$UID.pid`.
pub fn default_pid_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("nextmeeting.pid")
    } else {
        #[cfg(unix)]
        let uid = process::id();
        #[cfg(not(unix))]
        let uid = 0;
        PathBuf::from(format!("/tmp/nextmeeting-{}.pid", uid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn pidfile_create_and_remove() {
        let dir = tempdir().unwrap();
        let pid_path = dir.path().join("test.pid");

        {
            let pidfile = PidFile::create(&pid_path).unwrap();
            assert!(pid_path.exists());
            assert_eq!(pidfile.pid(), process::id());

            // Read the PID file and verify
            let contents = fs::read_to_string(&pid_path).unwrap();
            let stored_pid: u32 = contents.trim().parse().unwrap();
            assert_eq!(stored_pid, process::id());
        }

        // PID file should be removed on drop
        assert!(!pid_path.exists());
    }

    #[test]
    fn pidfile_rejects_duplicate() {
        let dir = tempdir().unwrap();
        let pid_path = dir.path().join("test.pid");

        let _pidfile1 = PidFile::create(&pid_path).unwrap();

        // Second attempt should fail
        let result = PidFile::create(&pid_path);
        assert!(matches!(result, Err(ServerError::AlreadyRunning { .. })));
    }

    #[test]
    fn pidfile_removes_stale() {
        let dir = tempdir().unwrap();
        let pid_path = dir.path().join("test.pid");

        // Write a stale PID (unlikely to be running)
        fs::write(&pid_path, "999999999\n").unwrap();

        // Should succeed by removing the stale file
        let pidfile = PidFile::create(&pid_path).unwrap();
        assert!(pid_path.exists());
        drop(pidfile);
    }

    #[test]
    fn pidfile_removes_invalid() {
        let dir = tempdir().unwrap();
        let pid_path = dir.path().join("test.pid");

        // Write invalid content
        fs::write(&pid_path, "not-a-pid\n").unwrap();

        // Should succeed by removing the invalid file
        let pidfile = PidFile::create(&pid_path).unwrap();
        assert!(pid_path.exists());
        drop(pidfile);
    }

    #[test]
    fn default_pid_path_format() {
        let path = default_pid_path();
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("nextmeeting"));
        assert!(path_str.ends_with(".pid"));
    }
}
