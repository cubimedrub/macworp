from sqlmodel import Field, SQLModel, Session, create_engine
from typing import Union
from .controllers import workflow

from fastapi import FastAPI
     
app = FastAPI()
app.include_router(workflow.router)


@app.get("/")
def read_root():
    return {"Hello": "World"}


@app.get("/items/{item_id}")
def read_item(item_id: int, q: Union[str, None] = None):
    return {"item_id": item_id, "q": q}