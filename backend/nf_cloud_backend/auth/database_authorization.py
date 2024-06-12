
from .abstract_authorization import AbstractAuthorization
from .login_request import LoginRequest
from ..models.user import User

from sqlmodel import Session


class DatabaseAuthorization(AbstractAuthorization):
    @classmethod
    def login(cls, provider_name: str, login_request: LoginRequest, session: Session) -> User:
        pass