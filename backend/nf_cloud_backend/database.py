from pathlib import Path
from typing import Annotated, Any, Set
from typing_extensions import Annotated


from fastapi import Depends
import os
from sqlalchemy import Engine
from sqlmodel import SQLModel, Session, create_engine, delete
import yaml 

from .models.prelude import *


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


def seed(path: Path):
    # Load seed data
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
