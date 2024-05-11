from pydantic import BaseModel, Field, EmailStr
from typing import Union
from typing import List
import datetime

class UserBaseSchema(BaseModel):
    email: EmailStr = Field(default=None)
    password: str = Field(default=None)

class UserLoginSchema(UserBaseSchema):
    class Config: ## TODO(chrohne): check, if this actually works!
        schema_extra = {
            "user" : {
                "email": "max@mustermann.de",
                "password" : "mustermann"
            }
        }

class UserRegisterSchema(UserBaseSchema):
    fullname: str = Field(default=None)
    class Config: ## TODO(chrohne): check, if this actually works!
        schema_extra = {
            "user" : {
                "email": "max@mustermann.de",
                "password" : "mustermann",
            }
        }
