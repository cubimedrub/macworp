use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::workflow_configuration::parameters::parameter::{
    IsParameter, IsParameterWithValue, Parameter,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PathList {
    #[serde(flatten)]
    inner: Parameter<Vec<PathBuf>>,
    /// Whether the parameter is exceptiung a file
    is_excepting_files: bool,
    /// Whether the parameter is excepting a folder
    is_excepting_folders: bool,
}

impl PathList {
    pub fn new(name: String, label: String, desc: String) -> Self {
        Self {
            inner: Parameter::new(name, label, desc),
            is_excepting_files: false,
            is_excepting_folders: false,
        }
    }

    pub fn is_excepting_files(&self) -> bool {
        self.is_excepting_files
    }

    pub fn is_excepting_files_mut(&mut self) -> &mut bool {
        &mut self.is_excepting_files
    }

    pub fn is_excepting_folders(&self) -> bool {
        self.is_excepting_folders
    }

    pub fn is_excepting_folders_mut(&mut self) -> &mut bool {
        &mut self.is_excepting_folders
    }
}

impl IsParameter for PathList {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn name_mut(&mut self) -> &mut String {
        self.inner.name_mut()
    }

    fn label(&self) -> &str {
        self.inner.label()
    }

    fn label_mut(&mut self) -> &mut String {
        self.inner.label_mut()
    }

    fn desc(&self) -> &str {
        self.inner.desc()
    }

    fn desc_mut(&mut self) -> &mut String {
        self.inner.desc_mut()
    }
}

impl IsParameterWithValue<Vec<PathBuf>> for PathList {
    fn value(&self) -> Option<&Vec<PathBuf>> {
        self.inner.value()
    }

    fn value_mut(&mut self) -> &mut Option<Vec<PathBuf>> {
        self.inner.value_mut()
    }
}

impl Default for PathList {
    fn default() -> Self {
        Self {
            inner: Parameter::default(),
            is_excepting_files: true,
            is_excepting_folders: false,
        }
    }
}
