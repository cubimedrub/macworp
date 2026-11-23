use serde::{Deserialize, Serialize};

use crate::workflow_configuration::parameters::parameter::{
    IsParameter, IsParameterWithValue, Parameter,
};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Text {
    #[serde(flatten)]
    inner: Parameter<String>,
    /// If multiline or not (only used fro rendering)
    is_multiline: bool,
}

impl Text {
    pub fn new(name: String, label: String, desc: String) -> Self {
        Self {
            inner: Parameter::new(name, label, desc),
            is_multiline: false,
        }
    }

    pub fn is_multiline(&self) -> bool {
        self.is_multiline
    }

    pub fn is_multiline_mut(&mut self) -> &mut bool {
        &mut self.is_multiline
    }
}

impl IsParameter for Text {
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

impl IsParameterWithValue<String> for Text {
    fn value(&self) -> Option<&String> {
        self.inner.value()
    }

    fn value_mut(&mut self) -> &mut Option<String> {
        self.inner.value_mut()
    }
}
