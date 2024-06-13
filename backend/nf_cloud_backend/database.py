from pathlib import Path
from typing import Annotated, Any, Set
from typing_extensions import Annotated


from fastapi import Depends
from sqlmodel import SQLModel, Session, create_engine, delete
import yaml 

from .models.prelude import *

engine = create_engine("postgresql+psycopg://postgres:developer@127.0.0.1:5434/nf_cloud", echo=True)

def get_session():
    with Session(engine) as session:
        yield session
        session.commit()

DbSession = Annotated[Session, Depends(get_session)]


def seed(session: Session, path: Path, drop_existing_data: bool):
    # Load seed data
    seeds: list[Any] = yaml.load(
        path.read_text(encoding="utf-8"), Loader=yaml.Loader
    )
    dropped_models: Set[type[SQLModel]] = set()
    for seed in seeds:
        model = model_from_string(seed["model"])
        # Check if records should be dropped
        if drop_existing_data and model not in dropped_models:
            dropped_models.add(model)
            session.exec(delete(model)) # type:ignore[call-overload]
        # Create record
        session.add(model(**seed["attributes"]))

    
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
