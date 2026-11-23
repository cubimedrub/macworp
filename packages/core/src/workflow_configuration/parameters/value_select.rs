use serde::{Deserialize, Serialize};

use crate::workflow_configuration::parameters::parameter::{
    IsParameter, IsParameterWithValue, Parameter,
};

/// One choice of a [`ValueSelect`]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SelectOption {
    /// Human readable label
    pub label: String,
    /// Value passed to the workflow engine
    pub value: String,
}

/// Choose one (or, with `is_multiselect`, several) of a fixed set of options
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ValueSelect {
    #[serde(flatten)]
    inner: Parameter<Vec<String>>,
    /// Allowed values
    #[serde(default)]
    options: Vec<SelectOption>,
    /// Whether more than one option can be selected
    #[serde(default)]
    is_multiselect: bool,
}

impl ValueSelect {
    pub fn new(name: String, label: String, desc: String) -> Self {
        Self {
            inner: Parameter::new(name, label, desc),
            options: Vec::new(),
            is_multiselect: false,
        }
    }

    pub fn options(&self) -> &[SelectOption] {
        &self.options
    }

    pub fn options_mut(&mut self) -> &mut Vec<SelectOption> {
        &mut self.options
    }

    pub fn is_multiselect(&self) -> bool {
        self.is_multiselect
    }

    pub fn is_multiselect_mut(&mut self) -> &mut bool {
        &mut self.is_multiselect
    }
}

impl IsParameter for ValueSelect {
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

impl IsParameterWithValue<Vec<String>> for ValueSelect {
    fn value(&self) -> Option<&Vec<String>> {
        self.inner.value()
    }

    fn value_mut(&mut self) -> &mut Option<Vec<String>> {
        self.inner.value_mut()
    }
}
