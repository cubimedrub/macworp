from sqlmodel import Session

from .login_request import LoginRequest
from ..models.user import User

class AbstractAuthorization:
    @classmethod
    def login(cls, provider_name: str, login_request: LoginRequest, session: Session) -> User:
        raise NotImplementedError("method should be implemented in subclass")
