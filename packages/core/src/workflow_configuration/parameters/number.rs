use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::workflow_configuration::parameters::parameter::{
    IsParameter, IsParameterWithValue, Parameter,
};

// TODO: Yes, we could make T implement num_trait to make it more specific
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(bound = "T: Serialize + DeserializeOwned + Clone + Copy")]
pub struct Number<T: Serialize + DeserializeOwned + Clone> {
    #[serde(flatten)]
    inner: Parameter<T>,
    min: Option<T>,
    max: Option<T>,
}

impl<T> Number<T>
where
    T: Serialize + DeserializeOwned + Clone + Copy,
{
    pub fn new(name: String, label: String, desc: String) -> Self {
        Self {
            inner: Parameter::new(name, label, desc),
            min: None,
            max: None,
        }
    }

    pub fn min(&self) -> Option<T> {
        self.min
    }

    pub fn min_mut(&mut self) -> &mut Option<T> {
        &mut self.min
    }

    pub fn max(&self) -> Option<T> {
        self.max
    }

    pub fn max_mut(&mut self) -> &mut Option<T> {
        &mut self.max
    }
}

impl<T> IsParameter for Number<T>
where
    T: Serialize + DeserializeOwned + Clone,
{
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

impl<T> IsParameterWithValue<T> for Number<T>
where
    T: Serialize + DeserializeOwned + Clone,
{
    fn value(&self) -> Option<&T> {
        self.inner.value()
    }

    fn value_mut(&mut self) -> &mut Option<T> {
        self.inner.value_mut()
    }
}

impl<T> Default for Number<T>
where
    T: Serialize + DeserializeOwned + Clone,
{
    fn default() -> Self {
        Self {
            inner: Parameter::<T>::default(),
            min: None,
            max: None,
        }
    }
}

pub type Integer = Number<i64>;

pub type Float = Number<f64>;
