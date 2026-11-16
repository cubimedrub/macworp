use std::path::{Path, PathBuf};

use macworp_core::workflow_configuration::render::render_parameter_file;

use super::{ExecutorError, Job};

/// The workflow parameters of a job as a file for the engine to read (`-params-file` for
/// Nextflow, `--configfile` for Snakemake), see [`render_parameter_file`] for why they do not
/// go on the command line. The file lives in a scratch directory of its own that is mounted
/// read-only into the engine container; [`super::run_spec`] removes it after the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterFile {
    /// Scratch directory holding the file, created for this run only
    directory: PathBuf,
    /// The JSON file inside `directory`
    file: PathBuf,
}

impl ParameterFile {
    /// Writes the workflow parameters of `job` as JSON into a new directory that every user can
    /// read (the engine runs as the owner of the project directory). `None` if no parameter has
    /// a value, then nothing is created.
    ///
    /// # Arguments
    /// * `job` - the job whose parameters are written
    pub fn write(job: &Job) -> Result<Option<Self>, ExecutorError> {
        let Some(values) = render_parameter_file(&job.configuration, job.reference_dir.as_deref())?
        else {
            return Ok(None);
        };
        let io = |error| ExecutorError::Io("the workflow parameters", error);
        let directory = tempfile::Builder::new()
            .prefix(&format!("macworp-run-{}-", job.run_id))
            .tempdir()
            .map_err(io)?
            .keep();
        let file = directory.join("parameters.json");
        let written = (|| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755))?;
            }
            std::fs::write(&file, values.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644))?;
            }
            Ok(())
        })();
        if let Err(error) = written {
            let _ = std::fs::remove_dir_all(&directory);
            return Err(io(error));
        }
        Ok(Some(Self { directory, file }))
    }

    /// The scratch directory, to mount read-only and to remove after the run.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The JSON file with the parameters.
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// Removes the scratch directory, for a launch that failed after [`Self::write`].
    pub fn discard(self) {
        let _ = std::fs::remove_dir_all(self.directory);
    }
}
