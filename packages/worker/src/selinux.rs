//! SELinux labels of the directories MAcWorP owns (storage root, desktop app folders).
//!
//! Containers the engine starts itself through the runtime socket (CWL tool containers, Nextflow
//! tasks) keep the daemon's default confinement, so on an enforcing host their bind mounts are
//! denied unless the host files carry the `container_file_t` type. Labelling only the folders
//! MAcWorP owns keeps the policy intact everywhere else: no daemon-wide switch, no relabelling of
//! folders the user picked.

use std::path::Path;
use std::process::Command;

/// The file type SELinux lets container processes read and write.
pub const CONTAINER_TYPE: &str = "container_file_t";

/// The SELinux label of one directory.
#[derive(Debug, Clone, Copy)]
pub struct DirectoryLabel<'a> {
    /// The directory, labels of files below it follow it when they are created
    path: &'a Path,
}

impl<'a> DirectoryLabel<'a> {
    /// Label handle for a directory.
    ///
    /// # Arguments
    /// * `path` - The directory
    pub fn of(path: &'a Path) -> Self {
        Self { path }
    }

    /// The full label (`user:role:type:level`) as `stat` prints it, `None` where it cannot be
    /// read (no SELinux, no GNU `stat`, missing directory).
    pub fn current(&self) -> Option<String> {
        let output = Command::new("stat")
            .args(["-c", "%C"])
            .arg(self.path)
            .output()
            .ok()?;
        let label = String::from_utf8(output.stdout).ok()?.trim().to_string();
        (output.status.success() && !label.is_empty() && label != "?").then_some(label)
    }

    /// Whether `label` has the [`CONTAINER_TYPE`] type (third field).
    ///
    /// # Arguments
    /// * `label` - Full label, see [`Self::current`]
    pub fn allows_containers(label: &str) -> bool {
        label.split(':').nth(2) == Some(CONTAINER_TYPE)
    }

    /// Labels the directory and everything below it [`CONTAINER_TYPE`] (`chcon -R`). Only the
    /// owner of the files may do this; files created below later inherit the type. Meant for
    /// folders the application created itself.
    pub fn apply(&self) -> std::io::Result<()> {
        let status = Command::new("chcon")
            .args(["-R", "-t", CONTAINER_TYPE])
            .arg(self.path)
            .status()?;
        status
            .success()
            .then_some(())
            .ok_or_else(|| std::io::Error::other(format!("chcon exited with {status}")))
    }

    /// Gives the directory and everything below it the label of its parent directory (`chcon -R
    /// --reference`), which is what a freshly created folder there has, undoing [`Self::apply`].
    /// `restorecon` is not used: it does nothing for folders the policy has no default for.
    pub fn restore(&self) -> std::io::Result<()> {
        let parent = self.path.parent().ok_or_else(|| {
            std::io::Error::other("the folder has no parent to take a label from")
        })?;
        let status = Command::new("chcon")
            .arg("-R")
            .arg(format!("--reference={}", parent.display()))
            .arg(self.path)
            .status()?;
        status
            .success()
            .then_some(())
            .ok_or_else(|| std::io::Error::other(format!("chcon exited with {status}")))
    }

    /// The commands an administrator runs to label the directory permanently, so it survives
    /// `restorecon` and relabelling of the file system.
    pub fn admin_command(&self) -> String {
        let path = self.path.display();
        format!(
            "sudo semanage fcontext -a -t {CONTAINER_TYPE} '{path}(/.*)?' && sudo restorecon -R '{path}'"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the type field decides, not user, role or level.
    #[test]
    fn container_type_is_the_third_field() {
        assert!(DirectoryLabel::allows_containers(
            "unconfined_u:object_r:container_file_t:s0"
        ));
        assert!(DirectoryLabel::allows_containers(
            "system_u:object_r:container_file_t:s0:c1,c2"
        ));
        assert!(!DirectoryLabel::allows_containers(
            "unconfined_u:object_r:user_home_t:s0"
        ));
        assert!(!DirectoryLabel::allows_containers(
            "container_file_t:object_r:user_home_t:s0"
        ));
        assert!(!DirectoryLabel::allows_containers(""));
    }

    /// The administrator command names the path for both the rule and the relabel.
    #[test]
    fn admin_command_names_the_directory() {
        let command = DirectoryLabel::of(Path::new("/srv/macworp")).admin_command();
        assert!(command.contains("'/srv/macworp(/.*)?'"));
        assert!(command.contains("restorecon -R '/srv/macworp'"));
    }
}
