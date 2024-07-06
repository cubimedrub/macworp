from contextlib import asynccontextmanager
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


@asynccontextmanager
async def lifespan(app: FastAPI):
    startup()
    yield
    shutdown()


def startup():
    """
    Executed when the app starts up.
    """
    SQLModel.metadata.create_all(engine)


def shutdown():
    """
    Executed when the app shuts down.
    """
    pass