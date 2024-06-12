from fastapi import FastAPI
from sqlmodel import SQLModel

from .controllers import users
from .controllers import project
from .controllers import workflow
from .database import engine, DbSession #Dbsession
from .models.user import User, UserRole #models
from .auth.password_handler import *

app = FastAPI()
app.include_router(project.router)
app.include_router(workflow.router)
app.include_router(users.router)


@app.on_event("startup")
def on_startup():
    SQLModel.metadata.create_all(engine)

@app.get("/")
def read_root():
    return {"Hello": "World"}


DbSession.add(User(id=1, login_id="testperson", role=UserRole.admin, provider_type="database", provider_name="dev", email="testperson@gmail.com", 
                   hashed_password=get_password_hash("testperson", )))
DbSession.commit()
