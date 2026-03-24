use macworp_ui_core::storage::IsStorage;
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum Error {}

#[derive(Debug, Clone)]
pub struct Storage {
    jwt: Option<String>,
}

impl Storage {
    pub fn new() -> Self {
        Self { jwt: None }
    }
}

impl IsStorage for Storage {
    type Error = Error;

    fn jwt(&self) -> Result<Option<String>, Self::Error> {
        Ok(self.jwt.clone())
    }

    fn set_jwt(&mut self, jwt: Option<String>) -> Result<(), Self::Error> {
        self.jwt = jwt;
        Ok(())
    }
}
