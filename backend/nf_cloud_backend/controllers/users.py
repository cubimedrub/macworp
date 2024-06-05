from sqlmodel import Field, SQLModel, Session, create_engine
from sqlalchemy.sql import text
from typing import Union

from ..models.workflow import Workflow
from ..models.user import User, UserRole
from ..models.workflow_share import WorkflowShare
from ..database import get_db
from ..auth.auth_handler import *
from ..models.schemas import *
from ..auth.authorization import *

from fastapi import FastAPI, Depends, HTTPException, status, APIRouter
from fastapi.security import OAuth2PasswordBearer, OAuth2PasswordRequestForm
from pydantic import BaseModel
from datetime import timedelta, datetime
from jose import JWTError, jwt
from typing_extensions import Annotated

router = APIRouter()

@router.post("/register", tags=["user"])
def register_user(user: UserRegisterSchema, db = Depends(get_db)):
    if not is_user_already_registered(db, user.login_id):
        db_user =  register_new_user(db, user)
        access_token_expires = timedelta(minutes=ACCESS_TOKEN_EXPIRE_MINUTES)
        token = create_access_token(data={"email": db_user.login_id}, expires_delta=access_token_expires)
        return {"access_token": token, "token_type": "bearer"}
    else:
        raise HTTPException(
        status_code=404,
        detail="User is already in registered!",
        headers={"WWW-Authenticate": "Bearer"},
    )

@router.post("/login", tags=["user"])
def login_user(form_data: OAuth2PasswordRequestForm = Depends(), db = Depends(get_db)):
    authenticated = authenticate_user(db, form_data.username, form_data.password)
    if authenticated:
        db_user = get_user_by_login_id(db, form_data.username)
        access_token_expires = timedelta(minutes=ACCESS_TOKEN_EXPIRE_MINUTES)
        token = create_access_token(data={"email": db_user.login_id}, expires_delta=access_token_expires)
        return {"access_token": token, "token_type": "bearer"}
    else:
        raise HTTPException(
        status_code=status.HTTP_401_UNAUTHORIZED,
        detail="Could not validate credentials",
        headers={"WWW-Authenticate": "Bearer"},
    )
