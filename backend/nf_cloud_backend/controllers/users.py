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
from ..auth.provider_type import ProviderType

from fastapi import FastAPI, Depends, HTTPException, status, APIRouter
from fastapi.security import OAuth2PasswordBearer, OAuth2PasswordRequestForm
from pydantic import BaseModel
from datetime import timedelta, datetime
from jose import JWTError, jwt
from typing_extensions import Annotated

router = APIRouter()

@router.post("/register/{provider_type}/{provider}", tags=["user"])
def register_user(user: UserRegisterSchema, provider_type: str, provider: str, db = Depends(get_db)):
    if provider_type == ProviderType.OPENID_CONNECT.value:

        if not Authorization.is_user_already_registered(db, user.login_id):
            db_user =  Authorization.register_new_user(db, user, provider_type, provider)
            access_token_expires = timedelta(minutes=ACCESS_TOKEN_EXPIRE_MINUTES)
            token = create_access_token(data={"email": db_user.login_id}, expires_delta=access_token_expires)
            return {"access_token": token, "token_type": "bearer"}

        else:
            raise HTTPException(
            status_code=404,
            detail="User is already in registered!",
            headers={"WWW-Authenticate": "Bearer"},
            )
        
    if provider_type == ProviderType.FILE.value:
            #Todo
            pass
    
    else:
        raise HTTPException(status_code=404, detail="Provider Type not found")

@router.post("/login/{provider_type}/{provider}", tags=["user"])
def login_user(provider_type: str, provider: str, form_data: OAuth2PasswordRequestForm = Depends(),  db = Depends(get_db)):
    if provider_type == ProviderType.OPENID_CONNECT.value:

        authenticated = Authorization.authenticate_user(db, form_data.username, form_data.password)
        if authenticated:
            db_user = Authorization.get_user_by_login_id(db, form_data.username)
            access_token_expires = timedelta(minutes=ACCESS_TOKEN_EXPIRE_MINUTES)
            token = create_access_token(data={"email": db_user.login_id}, expires_delta=access_token_expires)
            return {"access_token": token, "token_type": "bearer"}

        else:
            raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Could not validate credentials",
            headers={"WWW-Authenticate": "Bearer"},
        )
        
    if provider_type == ProviderType.FILE.value:
            #Todo
            pass
    
    else:
        raise HTTPException(status_code=404, detail="Provider Type not found")
