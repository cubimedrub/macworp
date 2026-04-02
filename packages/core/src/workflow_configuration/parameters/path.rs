use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::workflow_configuration::parameters::parameter::{
    IsParameter, IsParameterWithValue, Parameter,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Path {
    #[serde(flatten)]
    inner: Parameter<PathBuf>,
    /// Whether the parameter is exceptiung a file
    is_excepting_file: bool,
    /// Whether the parameter is excepting a folder
    is_excepting_folder: bool,
}

impl Path {
    pub fn new(name: String, label: String, desc: String) -> Self {
        Self {
            inner: Parameter::new(name, label, desc),
            is_excepting_file: false,
            is_excepting_folder: false,
        }
    }

    pub fn is_excepting_file(&self) -> bool {
        self.is_excepting_file
    }

    pub fn is_excepting_file_mut(&mut self) -> &mut bool {
        &mut self.is_excepting_file
    }

    pub fn is_excepting_folder(&self) -> bool {
        self.is_excepting_folder
    }

    pub fn is_excepting_folder_mut(&mut self) -> &mut bool {
        &mut self.is_excepting_folder
    }
}

impl IsParameter for Path {
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

impl IsParameterWithValue<PathBuf> for Path {
    fn value(&self) -> Option<&PathBuf> {
        self.inner.value()
    }

    fn value_mut(&mut self) -> &mut Option<PathBuf> {
        self.inner.value_mut()
    }
}

impl Default for Path {
    fn default() -> Self {
        Self {
            inner: Parameter::default(),
            is_excepting_file: true,
            is_excepting_folder: false,
        }
    }
}
