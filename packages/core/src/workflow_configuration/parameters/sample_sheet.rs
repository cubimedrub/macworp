use serde::{Deserialize, Serialize};

use crate::workflow_configuration::parameters::parameter::{
    IsParameter, IsParameterWithValue, Parameter,
};

/// Path of a sample sheet (CSV/TSV/YAML/JSON) inside the project.
///
/// STUB: only stores the path as text. The full version adds a table editor generated from the
/// sample sheet schema (`schema_input.json`), project file picker, R1/R2 pairing helpers,
/// import/export and validation. The schema is not stored here yet.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SampleSheet {
    #[serde(flatten)]
    inner: Parameter<String>,
}

impl SampleSheet {
    /// Creates a sample sheet parameter without a value.
    ///
    /// # Arguments
    /// * `name` - Name passed to the engine (without dashes)
    /// * `label` - Human readable label
    /// * `desc` - Description
    pub fn new(name: String, label: String, desc: String) -> Self {
        Self {
            inner: Parameter::new(name, label, desc),
        }
    }
}

impl IsParameter for SampleSheet {
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

impl IsParameterWithValue<String> for SampleSheet {
    fn value(&self) -> Option<&String> {
        self.inner.value()
    }

    fn value_mut(&mut self) -> &mut Option<String> {
        self.inner.value_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stub keeps name, label and value through JSON.
    #[test]
    fn serde_round_trip() {
        let mut sheet = SampleSheet::new("input".into(), "Sheet".into(), "d".into());
        *sheet.value_mut() = Some("samples.csv".into());
        let json = serde_json::to_string(&sheet).unwrap();
        assert_eq!(serde_json::from_str::<SampleSheet>(&json).unwrap(), sheet);
    }
}
