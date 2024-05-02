from typing import Annotated
from fastapi import Depends
from sqlalchemy import create_engine
from sqlmodel import SQLModel, Session

from .models.workflow import Workflow
from .models.user import User, UserRole
from .models.workflow_share import WorkflowShare

engine = create_engine("postgresql+psycopg://postgres:developer@127.0.0.1:5434/nf_cloud", echo=True)

SQLModel.metadata.create_all(engine)

def get_session():
    with Session(engine) as session:
        yield session
        session.commit()

DbSession = Annotated[Session, Depends(get_session)]
