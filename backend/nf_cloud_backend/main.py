from sqlmodel import Field, SQLModel, Session, create_engine
from sqlalchemy.sql import text
from typing import Union

from .auth.auth_handler import *
from .models.schemas import *
from .controllers import users

from fastapi import FastAPI, Depends, HTTPException, status
from pydantic import BaseModel
from datetime import timedelta, datetime
from jose import JWTError, jwt
from typing_extensions import Annotated

ALGORITHM = "HS256"
SECRET_KEY = "1381838ae617aecd50fe746b9095358e50a19de84f9a585f32d4a8138476082c" #generated with openssl rand -hex 32
ACCESS_TOKEN_EXPIRE_MINUTES = 30

oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")

app = FastAPI()

app.include_router(users.router)






