/// Filesystem errors
pub mod errors;

/// File operations confined to a base directory
pub mod directory_files;

/// Definition of a filesystem
pub mod is_filesystem;

#[cfg(feature = "integration")]
pub mod local_filesystem;

/// Access to remote filesystem is done via
#[cfg(feature = "standalone")]
pub mod remote_filesystem;

/// Project content
pub mod project_content;
