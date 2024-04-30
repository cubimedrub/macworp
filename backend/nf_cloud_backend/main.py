from sqlmodel import Field, SQLModel, Session, create_engine
from typing import Union
from .models.workflow import Workflow
from .models.user import User, UserRole
from .models.workflow_share import WorkflowShare

from fastapi import FastAPI, Depends, HTTPException, status
from fastapi.security import OAuth2PasswordBearer, OAuth2PasswordRequestForm
     
app = FastAPI()

engine = create_engine("postgresql+psycopg://postgres:developer@127.0.0.1:5434/nf_cloud", echo=True)

SQLModel.metadata.create_all(engine)

# quick test
with Session(engine) as session:
    session.add(User(role=UserRole.default, provider_type="file", provider_name="dev"))
    session.add(User(role=UserRole.admin, provider_type="openid_connect", provider_name="dev"))
    session.commit()

@app.get("/")
def read_root():
    return {"Hello": "World"}


@app.get("/items/{item_id}")
def read_item(item_id: int, q: Union[str, None] = None):
    return {"item_id": item_id, "q": q}

@app.get("/auth_test/")
async def auth_test():
    return {"auth": "test"}