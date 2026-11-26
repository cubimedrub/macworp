//! Keeps child processes from opening a console window on Windows.
//!
//! The desktop app is a window program, not a console program. Windows gives every console
//! program that such a process starts (`podman.exe`, `docker.exe`, `wsl.exe`, ...) a console window
//! of its own, which flashes up on screen for each call (the container runtime is queried often).
//! The process creation flag `CREATE_NO_WINDOW` prevents that. Everywhere else nothing changes.

/// Windows process creation flag `CREATE_NO_WINDOW`: the child gets no console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Adds the "no console window" flag to a command before it is started. Call it on every
/// command the worker runs; the output of the child is still captured through pipes or files.
pub trait HideConsoleWindow {
    /// Makes the child process open no console window on Windows, and does nothing on other
    /// systems. Returns the command for chaining.
    fn hide_console_window(&mut self) -> &mut Self;
}

impl HideConsoleWindow for tokio::process::Command {
    fn hide_console_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            self.creation_flags(CREATE_NO_WINDOW);
        }
        self
    }
}

impl HideConsoleWindow for std::process::Command {
    fn hide_console_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            self.creation_flags(CREATE_NO_WINDOW);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A command still runs and returns its output with the flag set (on Windows the flag is
    /// passed to process creation, elsewhere it is a no-op).
    #[tokio::test]
    async fn hidden_command_still_runs() {
        let program = if cfg!(windows) { "cmd" } else { "echo" };
        let arguments: &[&str] = if cfg!(windows) {
            &["/C", "echo hi"]
        } else {
            &["hi"]
        };
        let output = tokio::process::Command::new(program)
            .args(arguments)
            .hide_console_window()
            .output()
            .await
            .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("hi"));
        let output = std::process::Command::new(program)
            .args(arguments)
            .hide_console_window()
            .output()
            .unwrap();
        assert!(output.status.success());
    }
}
