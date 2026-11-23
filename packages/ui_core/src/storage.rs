use std::fmt::Debug;

pub trait IsStorage: Clone + Send + Sync + Debug {
    const JWT_KEY: &str = "jwt";
    /// Key under which the administrator's "Show all" choice is stored
    const SHOW_ALL_KEY: &str = "show_all";

    type Error: std::error::Error + Clone + Send + Sync;

    /// Returns the JWT
    fn jwt(&self) -> Result<Option<String>, Self::Error>;

    /// Sets the JWT
    fn set_jwt(&mut self, jwt: Option<String>) -> Result<(), Self::Error>;

    /// Whether the user chose to list all projects, workflows and groups (administrators only,
    /// see `ShowAll`). `false` if never set.
    fn show_all(&self) -> Result<bool, Self::Error>;

    /// Remembers the "Show all" choice
    fn set_show_all(&mut self, show_all: bool) -> Result<(), Self::Error>;
}
