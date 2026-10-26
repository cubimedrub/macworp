use serde::{Deserialize, Serialize};

use crate::workflow_configuration::parameters::parameter::{IsParameter, Parameter};

/// Visual divider with a label in the start dialog. Has no value and is never rendered into
/// engine arguments; `name` is unused.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Separator {
    #[serde(flatten)]
    inner: Parameter<String>,
}

impl Separator {
    pub fn new(label: String, desc: String) -> Self {
        Self {
            inner: Parameter::new(String::new(), label, desc),
        }
    }
}

impl IsParameter for Separator {
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
