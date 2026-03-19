pub trait IsStroage: Clone {
    type Error: std::error::Error;

    /// Returns the JWT
    fn jwt(&self) -> Result<Option<String>, Self::Error>;

    /// Sets the JWT
    fn set_jwt(&mut self, jwt: Option<String>) -> Result<(), Self::Error>;
}
