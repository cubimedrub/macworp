from .login_request import LoginRequest
from ..models.user import User

class AbstractAuthorization:
    @classmethod
    def login(cls, provider_name: str, login_request: LoginRequest) -> User:
        raise NotImplementedError("method should be implemented in subclass")
