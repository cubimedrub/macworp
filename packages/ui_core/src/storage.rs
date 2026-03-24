pub trait IsStorage: Clone {
    const JWT_KEY: &str = "jwt";

    type Error: std::error::Error + Clone;

    /// Returns the JWT
    fn jwt(&self) -> Result<Option<String>, Self::Error>;

    /// Sets the JWT
    fn set_jwt(&mut self, jwt: Option<String>) -> Result<(), Self::Error>;
}
