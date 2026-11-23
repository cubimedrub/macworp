use crate::workflow_configuration::parameters::parameter::{
    IsParameter, IsParameterWithValue, Parameter,
};

pub type Glob = Parameter<String>;

impl IsParameter for Glob {
    fn name(&self) -> &str {
        self.name()
    }

    fn name_mut(&mut self) -> &mut String {
        self.name_mut()
    }

    fn label(&self) -> &str {
        self.label()
    }

    fn label_mut(&mut self) -> &mut String {
        self.label_mut()
    }

    fn desc(&self) -> &str {
        self.desc()
    }

    fn desc_mut(&mut self) -> &mut String {
        self.desc_mut()
    }
}

impl IsParameterWithValue<String> for Glob {
    fn value(&self) -> Option<&String> {
        self.value()
    }

    fn value_mut(&mut self) -> &mut Option<String> {
        self.value_mut()
    }
}
