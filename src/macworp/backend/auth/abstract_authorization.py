from fastapi import FastAPI
from sqlmodel import Session

from macworp.configuration import Configuration

from .login_request import LoginRequest
from ..models.user import User


class AbstractAuthorization:
    @classmethod
    def login(
        cls,
        app: FastAPI,
        provider_name: str,
        login_request: LoginRequest,
        session: Session,
        config: Configuration,
    ) -> User:
        raise NotImplementedError("method should be implemented in subclass")
