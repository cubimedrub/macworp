import enum
from typing import TYPE_CHECKING

from datetime import timedelta, datetime
from jose import JWTError, jwt
from fastapi import FastAPI, Depends, HTTPException, status
from fastapi.security import OAuth2PasswordBearer, OAuth2PasswordRequestForm
from passlib.context import CryptContext
from pydantic import BaseModel
from sqlalchemy import Date, Time, Column, ForeignKey, Integer, String, Enum, Numeric, Boolean
from sqlalchemy.orm import relationship
from sqlmodel import Enum, Field, Relationship, SQLModel

if TYPE_CHECKING:
    from .workflow import Workflow
    from .workflow_share import WorkflowShare
    from .project import Project
    from .project_share import ProjectShare

class UserRole(str, enum.Enum):
    default = "default"
    admin = "admin"


class User(SQLModel, table=True):
    id: int | None = Field(default=None, primary_key=True)
    login_id: str | None = None
    role: UserRole = Enum(UserRole)
    provider_type: str
    provider_name: str
    email: str 
    hashed_password: str | None = None
    disabled: bool | None = None

    owned_workflows: list["Workflow"] = Relationship(back_populates="owner")
    owned_projects: list["Project"] = Relationship(back_populates="owner")

    workflow_shares: list["WorkflowShare"] = Relationship(back_populates="user")
    project_shares: list["ProjectShare"] = Relationship(back_populates="user")
