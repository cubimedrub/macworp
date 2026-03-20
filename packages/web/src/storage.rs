use macworp_ui_core::storage::IsStorage;
use thiserror::Error;

use web_sys::{window, Storage as LocalStorage};

const JWT_KEY: &str = "jwt";

/// Error for stroage access
#[derive(Debug, Error)]
pub enum Error {
    #[error("Unable to get browser window for storage access")]
    UnableToGetWindows,
    #[error("Failed to access local storage")]
    LocalStorageAccess,
    #[error("No local storage support, please update your browser.")]
    MissingLocalStorageSupport,
}

/// Wrapper around local storage for saving settings
#[derive(Debug, Clone)]
pub struct Storage {
    inner_storage: LocalStorage,
}

impl Storage {
    pub fn new() -> Result<Self, Error> {
        let storage = window()
            .ok_or(Error::UnableToGetWindows)?
            .local_storage()
            .map_err(|_| Error::LocalStorageAccess)?
            .ok_or(Error::MissingLocalStorageSupport)?;

        Ok(Self {
            inner_storage: storage,
        })
    }
}

impl IsStorage for Storage {
    type Error = Error;

    fn jwt(&self) -> Result<Option<String>, Self::Error> {
        self.inner_storage
            .get_item(JWT_KEY)
            .map_err(|_| Error::LocalStorageAccess)
    }

    fn set_jwt(&mut self, jwt: Option<String>) -> Result<(), Self::Error> {
        match jwt {
            Some(jwt) => self
                .inner_storage
                .set_item(JWT_KEY, &jwt)
                .map_err(|_| Error::LocalStorageAccess),
            None => self
                .inner_storage
                .remove_item(JWT_KEY)
                .map_err(|_| Error::LocalStorageAccess),
        }
    }
}
