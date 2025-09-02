from pathlib import Path
from typing import Annotated, Any, Optional

from fastapi import Depends
from sqlalchemy import Engine
from sqlmodel import SQLModel, create_engine, Session
import yaml

from macworp.configuration import Configuration

from .models.project import Project
from .models.project_share import ProjectShare
from .models.user import User
from .models.workflow import Workflow
from .models.workflow_share import WorkflowShare


global_db_engine: Optional[Engine] = None


def init_database(database_url: str):
    """
    Initializes the global session maker with the given database URL.
    """
    global global_db_engine

    if global_db_engine is None:
        global_db_engine = create_engine(database_url, echo=True)


def get_db_session():
    with Session(global_db_engine) as session:
        yield session
        session.commit()


DbSession = Annotated[Session, Depends(get_db_session)]
"""
FastAPI dependency to get a session and eventually commit it.
"""


def seed_database(config: Configuration, seeds_path: Path):
    """
    Seeds the database.

    The file pointed to by the given path is expected to be a YAML list of objects with keys "model" and "attributes".
    """
    init_database(config.backend.database)

    seeds: list[Any] = yaml.load(
        seeds_path.read_text(encoding="utf-8"), Loader=yaml.Loader
    )
    for seed in seeds:
        model = model_from_string(seed["model"])
        print("seeding ", seed)

        with Session(global_db_engine) as session:
            session.add(model(**seed["attributes"]))
            session.commit()


def init_database_schema(config: Configuration):
    """Creates the database schema. This is NOT a migration tool

    Parameters
    ----------
    config : Configuration
        _description_
    """
    from macworp.backend.models.user import User, UserRole
    from macworp.backend.models.project import Project
    from macworp.backend.models.project_share import ProjectShare
    from macworp.backend.models.workflow import Workflow
    from macworp.backend.models.workflow_share import WorkflowShare

    init_database(config.backend.database)

    print(f"DB Migration: {global_db_engine}")
    print("Creating database tables")

    print("Registered tables:")
    for table_name in SQLModel.metadata.tables.keys():
        print(f"  - {table_name}")

    SQLModel.metadata.create_all(global_db_engine)
    print("Database migration completed")


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
