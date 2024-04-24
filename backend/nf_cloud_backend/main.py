from sqlmodel import Field, SQLModel, create_engine
from typing import Union
from .models.user import User

from fastapi import FastAPI
     
app = FastAPI()

engine = create_engine("postgresql://postgres:developer@127.0.0.1:5434/nf_cloud", echo=True)

SQLModel.metadata.create_all(engine)

@app.get("/")
def read_root():
    return {"Hello": "World"}


@app.get("/items/{item_id}")
def read_item(item_id: int, q: Union[str, None] = None):
    return {"item_id": item_id, "q": q}