from functools import lru_cache
import os
from pathlib import Path
from typing import Dict, List, Self

from fastapi import FastAPI
from pydantic import BaseModel, model_validator
from pydantic_yaml import parse_yaml_raw_as
from sqlmodel import select, Session
import yaml

from macworp.backend.auth.provider_type import ProviderType
from macworp.configuration import Configuration
from macworp.backend.models.user import User, UserRole
from macworp.backend.auth.abstract_authorization import AbstractAuthorization
from macworp.backend.auth.login_request import LoginRequest

file_based_users = {}

if os.environ.get("USERS_FILES"):
    for user_file in os.environ["USERS_FILES"].split(","):
        with open(user_file, encoding="utf-8") as f:
            users_db = yaml.load(f, Loader=yaml.Loader)
            file_based_users[users_db["provider"]] = users_db["users"]


class FileUserRecord(BaseModel):
    password: str
    email: str
    role: str
    login_id: str


class FileUserRecordCollection(BaseModel):
    """
    Collection of user records from a file.
    """

    users: List[FileUserRecord]
    """Dictionary of users with their credentials."""

    _index: Dict[str, int] = {}

    def __getitem__(self, login_id: str) -> FileUserRecord:
        """
        Returns the user record for the given login_id.
        Raises KeyError if the login_id does not exist.
        """
        if login_id not in self._index:
            raise KeyError(f"User with login_id {login_id} not found")
        return self.users[self._index[login_id]]

    @model_validator(mode="after")
    def create_index(self) -> Self:
        """
        Creates an index for fast lookup of user records by login_id.
        """
        for idx, user in enumerate(self.users):
            self._index[user.login_id] = idx

        return self

    @classmethod
    def from_file(cls, path: Path) -> Self:
        """
        Loads the configuration from a file.

        Parameters
        ----------
        path : Path
            Path to the configuration YAML file.
        """
        if not path.is_file():
            raise ValueError(
                f"Configuration file {path} does not exist or is not a file."
            )
        return parse_yaml_raw_as(cls, path.read_text(encoding="utf-8"))


class FileBasedAuthorization(AbstractAuthorization):
    @classmethod
    def login(
        cls,
        app: FastAPI,
        provider_name: str,
        login_request: LoginRequest,
        session: Session,
        config: Configuration,
    ) -> User:

        provider = config.backend.login_providers.file.get(provider_name)

        if provider is None:
            raise ValueError(f"Provider {provider_name} not found")

        user_records = cls.get_file_user_collection(provider.file)

        user_record = user_records[login_request.login_id]

        if user_record is None:
            raise ValueError(f"User {login_request.login_id} not found")

        if user_record.password != login_request.password:
            raise ValueError("Invalid password")

        user = session.exec(
            select(User).where(User.login_id == login_request.login_id)
        ).one_or_none()
        if user is None:
            user = User(
                login_id=login_request.login_id,
                email=user_record.email,
                role=UserRole.from_str(user_record.role),
                provider_type=ProviderType.FILE.value,
                provider_name=provider_name,
                hashed_password=login_request.password,
            )
            session.add(user)
            session.commit()

        return user

    @classmethod
    @lru_cache()
    def get_file_user_collection(cls, records_path: Path) -> FileUserRecordCollection:
        """
        Returns the users from the file

        Parameters
        ----------
        records_path : Path
            Path to the file containing user records.
        """

        return FileUserRecordCollection.from_file(records_path)
