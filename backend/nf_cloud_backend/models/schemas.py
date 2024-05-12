from pydantic import BaseModel, Field, EmailStr
from typing import Union
from typing import List
from .user import UserRole
from sqlmodel import Enum
import datetime

class UserBaseSchema(BaseModel):
    email: EmailStr = Field(default=None)
    login_id: str = Field(default=None)
    password: str = Field(default=None)
    provider_type: str = Field(default=None)
    provider_name: str = Field(default=None)
    role: UserRole = Enum(UserRole)

class UserLoginSchema(UserBaseSchema):
    class Config: ## TODO(chrohne): check, if this actually works!
        schema_extra = {
            "user" : {
                "email": "max@mustermann.de",
                "password" : "mustermann"
            }
        }

class UserRegisterSchema(UserBaseSchema):
    #fullname: str = Field(default=None)
    class Config: ## TODO(chrohne): check, if this actually works!
        schema_extra = {
            "user" : {
                "email": "max@mustermann.de",
                "password" : "mustermann",
            }
        }
