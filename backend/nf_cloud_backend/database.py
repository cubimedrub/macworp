from sqlmodel import Field, SQLModel, Session, create_engine
from sqlalchemy.sql import text
from typing import Union

from pydantic import BaseModel
from typing_extensions import Annotated

engine = create_engine("postgresql+psycopg://postgres:developer@127.0.0.1:5434/nf_cloud", echo=True)

SQLModel.metadata.create_all(engine)

def get_db():
    with Session(engine) as session:
        yield session
        session.commit()