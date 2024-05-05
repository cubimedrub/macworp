import enum
from sqlmodel import Enum, Field, Relationship, SQLModel

from fastapi import FastAPI, Depends, HTTPException, status
from fastapi.security import OAuth2PasswordBearer, OAuth2PasswordRequestForm

from pydantic import BaseModel
from datetime import timedelta, datetime
from jose import JWTError, jwt
from passlib.context import CryptContext

class UserRole(str, enum.Enum):
	default = "default"
	admin = "admin"

# Note: "user" has special meaning in Postgres so when using psql it needs to be double-quoted!
class User(SQLModel, table=True):
	id: int | None = Field(default=None, primary_key=True)
	role: UserRole = Enum(UserRole)
	provider_type: str
	provider_name: str
	email: str or None = None
	hashed_password: str or None = None
	disabled: bool or None = None

	# shared_workflows: list["Workflow"] = Relationship(back_populates="shared_with", link_model="WorkflowShare")