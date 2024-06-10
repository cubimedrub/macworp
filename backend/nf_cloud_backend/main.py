from fastapi import FastAPI
from sqlmodel import SQLModel

from .controllers import users
from .controllers import project
from .controllers import workflow
from .database import engine

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
