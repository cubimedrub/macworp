from sqlmodel import Field, SQLModel, Session, create_engine
from sqlalchemy.sql import text
from typing import Union

from .models.workflow import Workflow
from .models.user import User, UserRole
from .models.workflow_share import WorkflowShare

from .auth.auth_handler import *
from .models.schemas import *
from .crud import *

from fastapi import FastAPI, Depends, HTTPException, status
from fastapi.security import OAuth2PasswordBearer, OAuth2PasswordRequestForm
from pydantic import BaseModel
from datetime import timedelta, datetime
from jose import JWTError, jwt
from typing_extensions import Annotated

ALGORITHM = "HS256"
SECRET_KEY = "1381838ae617aecd50fe746b9095358e50a19de84f9a585f32d4a8138476082c" #generated with openssl rand -hex 32
ACCESS_TOKEN_EXPIRE_MINUTES = 30
     
engine = create_engine("postgresql+psycopg://postgres:developer@127.0.0.1:5434/nf_cloud", echo=True)

SQLModel.metadata.create_all(engine)

# Inserting into DB Test
#with Session(engine) as session:
#    session.add(User(role=UserRole.default, provider_type="file", provider_name="dev", login_id="user_1"))
#    session.add(User(role=UserRole.admin, provider_type="openid_connect", provider_name="dev", login_id="user_2"))
#    statement = text('SELECT * FROM user')
#    users_db = session.execute(statement)
#    session.commit()

def get_db():
    with Session(engine) as session:
        yield session


oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")

app = FastAPI()


@app.get("/users", tags=["user"])
def get_all_users(token: str = Depends(oauth2_scheme), db = Depends(get_db)):
    return crud.get_all_users(db)

@app.get("/user_by_mail", tags=["user"])
def get_user(email: EmailStr, token: str = Depends(oauth2_scheme), db = Depends(get_db)):
    db_user = get_user_by_mail(db, email)
    if db_user:
        return db_user
    else:
        raise HTTPException(
        status_code=404,
        detail="No User found with {} email".format(email),
        headers={"WWW-Authenticate": "Bearer"},
    )


@app.post("/register", tags=["user"])
def register_user(user: UserRegisterSchema, db = Depends(get_db)):
    if not is_email_already_registered(db, user.email):
        db_user =  register_new_user(db, user)
        access_token_expires = timedelta(minutes=ACCESS_TOKEN_EXPIRE_MINUTES)
        token = create_access_token(data={"email": db_user.email}, expires_delta=access_token_expires)
        return {"access_token": token, "token_type": "bearer"}
    else:
        raise HTTPException(
        status_code=404,
        detail="Email is already in registered!",
        headers={"WWW-Authenticate": "Bearer"},
    )

@app.post("/login", tags=["user"])
def login_user(form_data: OAuth2PasswordRequestForm = Depends(), db = Depends(get_db)):
    authenticated = authenticate_user(db, form_data.username, form_data.password)
    if authenticated:
        db_user = get_user_by_mail(db, form_data.username)
        access_token_expires = timedelta(minutes=ACCESS_TOKEN_EXPIRE_MINUTES)
        token = create_access_token(data={"email": db_user.email}, expires_delta=access_token_expires)
        return {"access_token": token, "token_type": "bearer"}
    else:
        raise HTTPException(
        status_code=status.HTTP_401_UNAUTHORIZED,
        detail="Could not validate credentials",
        headers={"WWW-Authenticate": "Bearer"},
    )

@app.put("/user/change_email", tags=["user"])
def update_email(new_email: EmailStr, token: str = Depends(oauth2_scheme), db = Depends(get_db)):
    # Get mail from token:
    current_mail = extract_email_from_token(token)
    # Get User from db with current_mail
    db_user = get_user_by_mail(db, current_mail)
    if db_user:
        # return a new access token, because the email was updated!
        new_db_user = update_email(db, db_user, new_email)
        access_token_expires = timedelta(minutes=ACCESS_TOKEN_EXPIRE_MINUTES)
        token = create_access_token(data={"email": new_db_user.email}, expires_delta=access_token_expires)
        return {"access_token": token, "token_type": "bearer"}
    else:
        raise HTTPException(
        status_code=404,
        detail="User does not exist",
        headers={"WWW-Authenticate": "Bearer"},
    )

@app.put("/user/change_pwd", tags=["user"])
def update_password(new_password: str, old_password:str, token: str = Depends(oauth2_scheme), db = Depends(get_db)):
    # Get mail from token:
    current_mail = extract_email_from_token(token)
    authenticated = crud.authenticate_user(db, current_mail, old_password)
    if authenticated:
        # Get User from db with current_mail
        db_user = crud.get_user_by_mail(db, current_mail)
        if db_user:
            return crud.update_password(db, db_user, new_password)
        else:
            raise HTTPException(
                status_code=404,
                detail="User does not exist",
                headers={"WWW-Authenticate": "Bearer"})
    else:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Could not validate credentials",
            headers={"WWW-Authenticate": "Bearer"})

@app.delete("/user", tags=["user"])
def delete_current_user(password: str, token: str = Depends(oauth2_scheme), db = Depends(get_db)):
    # Get mail from token:
    mail = extract_email_from_token(token)
    authenticated = crud.authenticate_user(db, mail, password)
    if authenticated:
        db_user = crud.get_user_by_mail(db, mail)
        if db_user:
            if crud.remove_current_user(db, db_user):
                return {"message": "User with mail {} removed".format(mail)}
            else:
                raise HTTPException(
                            status_code=404,
                            detail="Could not remove current user, please contact admin",
                            headers={"WWW-Authenticate": "Bearer"})
        else:
            raise HTTPException(
                status_code=404,
                detail="Could not remove current user, please contact admin",
                headers={"WWW-Authenticate": "Bearer"})
    else:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Could not validate credentials",
            headers={"WWW-Authenticate": "Bearer"})