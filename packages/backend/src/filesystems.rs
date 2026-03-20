/// Filesystem errors
pub mod errors;

/// Definition of a filesystem
pub mod is_filesystem;

#[cfg(feature = "integration")]
pub mod local_filesystem;

/// Access to remote filesystem is done via
#[cfg(feature = "standalone")]
pub mod remote_filesystem;

/// Project content
pub mod project_content;
