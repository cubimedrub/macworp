use crate::authentication::error::Error;

pub fn verify_password(password: &[u8], hash: &str, user_id: i64) -> Result<bool, Error> {
    match password_auth::verify_password(password, hash) {
        Ok(_) => Ok(true),
        Err(password_auth::VerifyError::PasswordInvalid) => Err(Error::InvalidUserOrPassword),
        Err(password_auth::VerifyError::Parse(err)) => {
            Err(Error::InvalidPasswordHash(user_id, err))
        }
    }
}
