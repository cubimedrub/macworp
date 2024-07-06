from pathlib import Path
from typing import Annotated, Any, Set
from typing_extensions import Annotated
import os

from fastapi import Depends
from sqlalchemy import Engine
from sqlmodel import SQLModel, Session, create_engine, delete
import yaml

from .models.project import Project
from .models.project_share import ProjectShare
from .models.user import User
from .models.workflow import Workflow
from .models.workflow_share import WorkflowShare


def reset_engine():
    global engine

    MACWORP_DB_URL = os.getenv("MACWORP_DB_URL")
    if MACWORP_DB_URL is None:
        raise RuntimeError("MACWORP_DB_URL not set")

    if engine is not None:
        engine.dispose()
    engine = create_engine(MACWORP_DB_URL, echo=True)


engine = None
reset_engine()


def get_session():
    with Session(engine) as session:
        yield session
        session.commit()

DbSession = Annotated[Session, Depends(get_session)]
"""
FastAPI dependency to get a session and eventually commit it.
"""


def seed(path: Path):
    """
    Seeds the database.

    The file pointed to by the given path is expected to be a YAML list of objects with keys "model" and "attributes".
    """
    
    seeds: list[Any] = yaml.load(
        path.read_text(encoding="utf-8"), Loader=yaml.Loader
    )
    for seed in seeds:
        model = model_from_string(seed["model"])
        print("seeding ", seed)
        
        with Session(engine) as session:
            session.add(model(**seed["attributes"]))
            session.commit()

    
def model_from_string(string: str) -> type[SQLModel]:
    """
    Gets a model from its class name.
    """

    match string:
        case "Project":
            return Project
        case "ProjectShare":
            return ProjectShare
        case "User":
            return User
        case "Workflow":
            return Workflow
        case "WorkflowShare":
            return WorkflowShare
    raise RuntimeError(f"Model {string} not found")
