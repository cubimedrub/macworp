import os

from sqlmodel import Session, select
import yaml

from ..auth.provider_type import ProviderType
from ..models.user import User, UserRole
from .abstract_authorization import AbstractAuthorization
from .login_request import LoginRequest
from ..database import engine

file_based_users = {}

if os.environ.get("USERS_FILES"):
    for user_file in os.environ["USERS_FILES"].split(","):
        with open(user_file, encoding="utf-8") as f:
            users_db = yaml.load(f, Loader=yaml.Loader)
            file_based_users[users_db['provider']] = users_db['users']

class FileBasedAuthorization(AbstractAuthorization):
    @classmethod
    def login(cls, provider_name: str, login_request: LoginRequest, session: Session) -> User:
        provider = file_based_users.get(provider_name)
        if provider is None:
            raise ValueError(f"Provider {provider_name} not found")
        
        file_record = provider.get(login_request.login_id)
        if file_record is None:
            raise ValueError(f"User {login_request.login_id} not found")
        
        if file_record["password"] != login_request.password:
            raise ValueError("Invalid password")
        
        user = session.exec(select(User).where(User.login_id == login_request.login_id)).one_or_none()
        if user is None:
            user = User(
                login_id=login_request.login_id,
                email=file_record["email"],
                role=UserRole.from_str(file_record["role"]),
                provider_type=ProviderType.FILE.value, 
                provider_name=provider_name,
                hashed_password=login_request.password,
                disabled=False
            )
            session.add(user)
            session.commit()

        return user
