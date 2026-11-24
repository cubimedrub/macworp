use std::path::{Path, PathBuf};

use macworp_core::{paths::PathMapping, workflow_configuration::render::render_parameter_file};

use super::{ExecutorError, Job};

/// The workflow parameters of a job as a file for the engine to read (`-params-file` for
/// Nextflow, `--configfile` for Snakemake), see [`render_parameter_file`] for why they do not
/// go on the command line. The file lives in a scratch directory of its own that is mounted
/// read-only into the engine container; [`super::run_spec`] removes it after the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterFile {
    /// Scratch directory holding the file, created for this run only
    directory: PathBuf,
    /// The JSON file inside `directory`, only written when `has_values`
    file: PathBuf,
    /// Whether any parameter had a value, so `file` exists
    has_values: bool,
    /// Further files written next to it by [`Self::write_with`], in the order given
    extra_files: Vec<PathBuf>,
}

impl ParameterFile {
    /// Writes the workflow parameters of `job` as JSON into a new directory that every user can
    /// read (the engine runs as the owner of the project directory). `None` if no parameter has
    /// a value, then nothing is created.
    ///
    /// # Arguments
    /// * `job` - the job whose parameters are written
    /// * `mapping` - how host paths look inside the runtime's containers
    pub fn write(job: &Job, mapping: PathMapping) -> Result<Option<Self>, ExecutorError> {
        Self::write_with(job, mapping, &[])
    }

    /// Like [`Self::write`], with further small files in the same scratch directory (for
    /// example an engine configuration the worker generates). The directory is created when
    /// there are parameter values or extra files, so [`Self::has_values`] says whether
    /// [`Self::file`] exists.
    ///
    /// # Arguments
    /// * `job` - the job whose parameters are written
    /// * `mapping` - how host paths look inside the runtime's containers
    /// * `extra_files` - `(file name, content)` pairs, the names are plain file names
    pub fn write_with(
        job: &Job,
        mapping: PathMapping,
        extra_files: &[(&str, String)],
    ) -> Result<Option<Self>, ExecutorError> {
        let values = render_parameter_file(
            &job.configuration,
            job.reference_dir.as_deref(),
            Some(&job.project_dir),
            mapping,
        )?;
        if values.is_none() && extra_files.is_empty() {
            return Ok(None);
        }
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
            let mut paths = Vec::new();
            if let Some(values) = &values {
                paths.push((file.clone(), values.to_string()));
            }
            for (name, content) in extra_files {
                paths.push((directory.join(name), content.clone()));
            }
            for (path, content) in paths {
                std::fs::write(&path, content)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
                }
            }
            Ok(())
        })();
        if let Err(error) = written {
            let _ = std::fs::remove_dir_all(&directory);
            return Err(io(error));
        }
        let extra_files = extra_files
            .iter()
            .map(|(name, _)| directory.join(name))
            .collect();
        Ok(Some(Self {
            has_values: values.is_some(),
            directory,
            file,
            extra_files,
        }))
    }

    /// The scratch directory, to mount read-only and to remove after the run.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The JSON file with the parameters, it only exists when [`Self::has_values`].
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// Whether any parameter had a value and [`Self::file`] was written.
    pub fn has_values(&self) -> bool {
        self.has_values
    }

    /// Path of the extra file `name` given to [`Self::write_with`], `None` if there was none.
    ///
    /// # Arguments
    /// * `name` - the file name given to [`Self::write_with`]
    pub fn extra_file(&self, name: &str) -> Option<&Path> {
        self.extra_files
            .iter()
            .find(|path| path.file_name().is_some_and(|file| file == name))
            .map(PathBuf::as_path)
    }

    /// Removes the scratch directory, for a launch that failed after [`Self::write`].
    pub fn discard(self) {
        let _ = std::fs::remove_dir_all(self.directory);
    }
}
