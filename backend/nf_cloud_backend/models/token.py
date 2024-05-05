from sqlmodel import SQLModel, Field
from fastapi import FastAPI, Depends, HTTPException, status
from fastapi.security import OAuth2PasswordBearer, OAuth2PasswordRequestForm

from pydantic import BaseModel
from datetime import timedelta, datetime
from jose import JWTError, jwt
from passlib.context import CryptContext

class Token(SQLModel, table=True):
    access_token: str or None = Field(default=None, primary_key=True)
    token_type: str