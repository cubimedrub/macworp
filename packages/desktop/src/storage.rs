use macworp_ui_core::storage::IsStorage;
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum Error {}

#[derive(Debug, Clone)]
pub struct Storage {
    jwt: Option<String>,
    /// Kept for the trait only: the desktop has no sharing, so nothing lists "all"
    show_all: bool,
}

impl Storage {
    pub fn new() -> Self {
        Self {
            jwt: None,
            show_all: false,
        }
    }
}

impl IsStorage for Storage {
    type Error = Error;

    fn jwt(&self) -> Result<Option<String>, Self::Error> {
        Ok(self.jwt.clone())
    }

    fn show_all(&self) -> Result<bool, Self::Error> {
        Ok(self.show_all)
    }

    fn set_show_all(&mut self, show_all: bool) -> Result<(), Self::Error> {
        self.show_all = show_all;
        Ok(())
    }

    fn set_jwt(&mut self, jwt: Option<String>) -> Result<(), Self::Error> {
        self.jwt = jwt;
        Ok(())
    }
}
