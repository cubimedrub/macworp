from sqlalchemy.orm import Session
from fastapi import HTTPException
from sqlalchemy import or_, and_

from .models.user import User
from .utils import create_timestamp
from .models.schemas import UserRegisterSchema
from .auth.password_handler import verify_password, get_password_hash


def get_all_users(db: Session, limit: int = 100):
    return db.query(User).all()

def get_user_by_mail(db: Session, email_query: str):
    return db.query(User).filter_by(email = email_query).first()

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

def is_email_already_registered(db: Session, email: str):
    db_user = get_user_by_mail(db, email)
    if db_user:
        return True
    else:
        return False

def authenticate_user(db: Session, email: str, password:str):
    db_user = get_user_by_mail(db, email)
    if not db_user:
        return False
    return verify_password(password, db_user.hashed_password)

def update_password(db: Session, db_user:User, new_password:str):
    hashed_password = get_password_hash(new_password)
    db_user.hashed_password = hashed_password
    db.add(db_user)
    db.commit()
    db.refresh(db_user)
    return db_user

def update_email(db: Session, db_user:User, new_email:str):
    db_user.email = new_email
    db.add(db_user)
    db.commit()
    db.refresh(db_user)
    return db_user

def remove_current_user(db: Session, db_user: User):
    successful = db.query(User).filter_by(id = db_user.id).delete()
    if successful:
        db.commit()
        return True
    else:
        return False