use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub trait IsParameter {
    fn name(&self) -> &str;
    fn name_mut(&mut self) -> &mut String;
    fn label(&self) -> &str;
    fn label_mut(&mut self) -> &mut String;
    fn desc(&self) -> &str;
    fn desc_mut(&mut self) -> &mut String;
}

pub trait IsParameterWithValue<T>
where
    T: Serialize + DeserializeOwned + Clone,
{
    fn value(&self) -> Option<&T>;
    fn value_mut(&mut self) -> &mut Option<T>;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(bound = "T: Serialize + DeserializeOwned + Clone")]
pub struct Parameter<T: Serialize + DeserializeOwned + Clone> {
    /// Name of the paramter as passed to the workflow engine (without any dashes)
    name: String,
    /// Human readable label
    label: String,
    /// Description
    desc: String,
    /// Description
    value: Option<T>,
}

impl<T> Parameter<T>
where
    T: Serialize + DeserializeOwned + Clone,
{
    pub fn new(name: String, label: String, desc: String) -> Self {
        Self {
            name,
            label,
            desc,
            value: None,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn label_mut(&mut self) -> &mut String {
        &mut self.label
    }

    pub fn desc(&self) -> &str {
        &self.desc
    }

    pub fn desc_mut(&mut self) -> &mut String {
        &mut self.desc
    }

    pub fn value(&self) -> Option<&T> {
        self.value.as_ref()
    }

    pub fn value_mut(&mut self) -> &mut Option<T> {
        &mut self.value
    }
}

impl<T> Default for Parameter<T>
where
    T: Serialize + DeserializeOwned + Clone,
{
    fn default() -> Self {
        Self {
            name: String::new(),
            label: String::new(),
            desc: String::new(),
            value: None,
        }
    }
}
