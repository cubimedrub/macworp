from sqlalchemy.orm import Session
from fastapi import HTTPException
from sqlalchemy import or_, and_

from ..models.user import User
from ..utils import create_timestamp
from ..models.schemas import UserRegisterSchema
from ..auth.password_handler import verify_password, get_password_hash


def get_all_users(db: Session, limit: int = 100):
    return db.query(User).all()

def get_user_by_login_id(db: Session, login_id_query: str):
    return db.query(User).filter_by(login_id = login_id_query).first()

def register_new_user(db: Session, new_user: UserRegisterSchema):
    hashed_password = get_password_hash(new_user.password)
    db_user = User(
        email=(new_user.email).lower(),
        hashed_password=hashed_password,
        login_id=new_user.login_id,
        provider_type=new_user.provider_type,
        provider_name=new_user.provider_name,
        role=new_user.role)
    db.add(db_user)
    db.commit()
    db.refresh(db_user)
    return db_user

def is_user_already_registered(db: Session, login_id: str):
    db_user = get_user_by_login_id(db, login_id)
    if db_user:
        return True
    else:
        return False

def authenticate_user(db: Session, login_id: str, password:str):
    db_user = get_user_by_login_id(db, login_id)
    if not db_user:
        return False
    return verify_password(password, db_user.hashed_password)
