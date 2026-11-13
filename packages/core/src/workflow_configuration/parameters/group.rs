use serde::{Deserialize, Serialize};

use crate::workflow_configuration::parameters::parameter::{IsParameter, Parameter};

/// Header of a group of parameters: the parameters after it, up to the next group, are shown
/// together in a (collapsible) frame. Has no value and is never rendered into engine arguments;
/// `name` is unused.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Group {
    #[serde(flatten)]
    inner: Parameter<String>,
}

impl Group {
    /// Creates a group header.
    ///
    /// # Arguments
    /// * `label` - Title of the group
    /// * `desc` - Description shown under the title
    pub fn new(label: String, desc: String) -> Self {
        Self {
            inner: Parameter::new(String::new(), label, desc),
        }
    }
}

impl IsParameter for Group {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow_configuration::parameters::Parameter as AnyParameter;

    /// Definitions stored before the rename (`Separator`) still load as groups.
    #[test]
    fn old_separator_name_is_accepted() {
        let json = r#"{"Separator": {"name": "", "label": "L", "desc": "", "value": null}}"#;
        let parameter: AnyParameter = serde_json::from_str(json).unwrap();
        assert_eq!(
            parameter,
            AnyParameter::Group(Group::new("L".into(), "".into()))
        );
    }
}
