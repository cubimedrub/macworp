"""
Endpoints

    /new - Creates a new empty workflow with name, description
    /:id/delete
        Only owner
    /:id/edit - Edit a existing workflow, possible attributes name, description, definition, is_published)
        Owner and others with write access
        All admins
    / - list all workflows
        Only published, shared and owned
            Except admin role => sees everything
    /:id - Show
        Owner and others with read or write access
        All admins
    /:id/share/add - shares workflows with user (read [default] or wrtite access)
    /:id/share/remove - remove share with a user
"""

from fastapi import APIRouter, Body, Depends, HTTPException, Header
from pydantic import BaseModel, Field
from sqlalchemy import select

from ..models.workflow import Workflow

from ..database import DbSession


router = APIRouter(
    prefix="/workflow"
)


@router.get("/")
async def list(session: DbSession) -> list[Workflow]:
    print(session.exec(select(Workflow)).all())
    return session.exec(select(Workflow)).all()


class WorkflowCreateParams(BaseModel):
    name: str
    description: str = "",
    definition: dict = Field(default_factory=dict)
    is_published: bool = False

@router.post("/new")
async def new(session: DbSession, params: WorkflowCreateParams):
    workflow = Workflow(name=params.name, description=params.description, definition=params.definition, is_published=params.is_published)
    session.add(workflow)
    session.commit()
    return workflow.id


@router.get("/{id}")
async def show(id: int, session: DbSession) -> Workflow | None:
    return session.get(Workflow, id)


class WorkflowUpdateParams(BaseModel):
    name: str | None = None
    description: str | None = None
    definition: dict | None = None
    is_published: bool | None = None

@router.post("/{id}/edit")
async def edit(id: int, params: Workflow, session: DbSession):
    workflow = session.get(Workflow, id)
    if params.name is not None:
        workflow.name = params.name
    if params.description is not None:
        workflow.description = params.description
    if params.definition is not None:
        workflow.definition = params.definition
    if params.is_published is not None:
        workflow.is_published = params.is_published


@router.post("/{id}/delete")
async def delete(id: int, session: DbSession):
    workflow: Workflow | None = session.get(Workflow, id)
    if workflow is None:
        raise HTTPException(detail="workflow not found")
    session.delete(workflow)

