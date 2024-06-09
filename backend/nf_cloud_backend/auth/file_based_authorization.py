from sqlalchemy.orm import Session
from fastapi import HTTPException
from sqlalchemy import or_, and_

from ..models.user import User
from ..utils import create_timestamp
from ..models.schemas import UserRegisterSchema
from ..auth.password_handler import verify_password, get_password_hash

class FileBasedAuthorization:
    pass