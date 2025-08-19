from fastapi import FastAPI
from sqlmodel import select, Session

from macworp.configuration import Configuration
from .abstract_authorization import AbstractAuthorization
from .login_request import LoginRequest
from ..models.user import User
from .password_handler import Hasher
from .provider_type import ProviderType


class DatabaseAuthorization(AbstractAuthorization):
    @classmethod
    def login(
        cls,
        app: FastAPI,
        provider_name: str,
        login_request: LoginRequest,
        session: Session,
        config: Configuration,
    ) -> User:

        statement = select(User).where(User.login_id == login_request.login_id)
        db_user = session.exec(statement).first()

        if db_user is None:
            raise ValueError(f"User {login_request.login_id} not found")

        if db_user.provider_type != ProviderType.DATABASE.value:
            raise ValueError("Provider Type not Database")

        if provider_name != "local" or db_user.provider_name != "local":
            raise ValueError("Not local")

        if not Hasher.verify_password(login_request.password, db_user.hashed_password):
            raise ValueError("Invalid password")

        user = session.exec(
            select(User).where(User.login_id == login_request.login_id)
        ).one_or_none()

        return user
