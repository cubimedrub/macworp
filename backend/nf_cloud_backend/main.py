from contextlib import asynccontextmanager
from fastapi import FastAPI
from sqlmodel import SQLModel
from fastapi.middleware.cors import CORSMiddleware

from .controllers import users
from .controllers import project
from .controllers import workflow
from .database import engine, DbSession
from .auth.password_handler import *

@asynccontextmanager
async def lifespan(app: FastAPI):
    print("Starting")
    from .models.user import User, UserRole
    from .models.project import Project
    from .models.project_share import ProjectShare
    from .models.workflow import Workflow
    from .models.workflow_share import WorkflowShare

    print(f" Engine: {engine}")
    print("Creating database tables")

    print("Registered tables:")
    for table_name in SQLModel.metadata.tables.keys():
        print(f"  - {table_name}")

    SQLModel.metadata.create_all(engine)
    print("Database migration completed")
    yield
    print("Shutting down...")

app = FastAPI(lifespan=lifespan)

app.include_router(project.router)
app.include_router(workflow.router)
app.include_router(users.router)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:8080", "http://127.0.0.1:3000"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)