use macworp_ui_core::storage::IsStroage;

#[derive(Debug, Clone)]
pub struct Storage {
    jwt: Option<String>,
}

impl Storage {
    pub fn new() -> Self {
        Self { jwt: None }
    }
}

impl IsStroage for Storage {
    type Error = ();

    fn jwt(&self) -> Result<Option<String>, Self::Error> {
        Ok(self.jwt)
    }

    fn set_jwt(&mut self, jwt: Option<String>) -> Result<(), Self::Error> {
        self.jwt = jwt;
        Ok(())
    }
}
