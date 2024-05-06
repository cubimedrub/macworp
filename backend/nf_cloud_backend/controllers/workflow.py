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

from typing import Annotated
from fastapi import APIRouter, Body, Depends, HTTPException, Header, Response, status
from pydantic import BaseModel, Field
from sqlalchemy import select
from sqlmodel import SQLModel

from .depends import ExistingUser, ExistingWorkflow

from ..models.user import User
from ..models.workflow import Workflow
from ..models.workflow_share import WorkflowShare

from ..database import DbSession


router = APIRouter(
    prefix="/workflow"
)


@router.get("/",
            summary="List Workflows")
async def list(session: DbSession) -> list[int]:
    """
    Lists the IDs of all workflows visible to this user.
    """

    return [i[0] for i in session.exec(select(Workflow.id)).all()]


class WorkflowCreateParams(BaseModel):
    name: str
    description: str = ""
    definition: dict = Field(default_factory=dict)
    is_published: bool = False

@router.post("/new",
             summary="Create Workflow")
async def new(params: WorkflowCreateParams, session: DbSession, response: Response) -> int:
    """
    Creates a new workflow with the provided parameters.

    For non-admins, the owner id has to be the currently logged in user.
    """

    workflow = Workflow(
        name=params.name,
        description=params.description,
        definition=params.definition,
        is_published=params.is_published
    )
    session.add(workflow)
    session.commit()
    # response.status_code = status.HTTP_201_CREATED
    return workflow.id


class WorkflowShown(BaseModel):
    name: str
    description: str
    definition: dict
    is_published: bool

@router.get("/{workflow_id}",
            summary="Show Single Workflow")
async def show(workflow: ExistingWorkflow) -> WorkflowShown:
    """
    Displays information for a single workflow.
    """

    return WorkflowShown(
        name=workflow.name,
        description=workflow.description,
        definition=workflow.definition,
        is_published=workflow.is_published
    )


class WorkflowUpdateParams(BaseModel):
    name: str | None = None
    description: str | None = None
    definition: dict | None = None
    is_published: bool | None = None

@router.post("/{workflow_id}/edit",
             summary="Edit Workflow")
async def edit(params: WorkflowUpdateParams, workflow: ExistingWorkflow) -> None:
    """
    Edits the attributes of a workflow.
    """

    if params.name is not None:
        workflow.name = params.name
    if params.description is not None:
        workflow.description = params.description
    if params.definition is not None:
        workflow.definition = params.definition
    if params.is_published is not None:
        workflow.is_published = params.is_published


@router.post("/{workflow_id}/delete",
             summary="Delete Workflow")
async def delete(workflow: ExistingWorkflow, session: DbSession) -> None:
    """
    Deletes a workflow.
    """

    session.delete(workflow)


@router.post("/{workflow_id}/share/add",
             summary="Share Workflow")
async def add_share(write: bool, workflow: ExistingWorkflow, user: ExistingUser, session: DbSession) -> None:
    """
    Gives read/write rights to a user, or changes the user's current rights.
    """

    share: WorkflowShare | None = session.exec(
        select(WorkflowShare).where(WorkflowShare.user_id == user.id)
    ).one_or_none()[0]
    print(share)

    
    if share is None:
        workflow.shares.append(WorkflowShare(
            user_id=user.id,
            workflow_id=workflow.id,
            write=write
        ))
    else:
        share.write = write


@router.post("/{workflow_id}/share/remove",
             summary="Un-Share Workflow")
async def remove_share(workflow: ExistingWorkflow, user: ExistingUser, session: DbSession) -> None:
    """
    Revokes the right of a user to read from / write to this workflow.
    """

    share: WorkflowShare | None = session.exec(
        select(WorkflowShare).where(WorkflowShare.user_id == user.id)
    ).one_or_none()[0]

    if share is not None:
        session.delete(share)