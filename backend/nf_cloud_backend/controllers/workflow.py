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
from sqlmodel import SQLModel, Session

from .depends import Authenticated, AuthenticatedUser, ExistingUser, ExistingWorkflow

from ..models.user import User, UserRole
from ..models.workflow import Workflow
from ..models.workflow_share import WorkflowShare

from ..database import DbSession


router = APIRouter(
    prefix="/workflow"
)


def get_workflow_share(user: User, workflow: Workflow, session: Session) -> WorkflowShare | None:
    return session.exec(
        select(WorkflowShare).where(WorkflowShare.user_id == user.id, WorkflowShare.workflow_id == workflow.id)
    ).one_or_none()[0]


def can_access_workflow(user: User, workflow: Workflow, session: Session, for_writing: bool = False) -> bool:
    match user.role:
        case UserRole.admin:
            return True
        case UserRole.default:
            if workflow.owner_id == user.id:
                return True
            share = get_workflow_share(user, workflow, session)
            return share is not None and (not for_writing or share.write)


@router.get("/",
            summary="List Workflows")
async def list(session: DbSession, auth: Authenticated) -> list[int]:
    """
    Lists the IDs of all workflows visible to this user.
    """
    
    return [
        i[0]
        for i in session.exec(select(Workflow)).all()
        if can_access_workflow(auth, i[0], session)
    ]


class WorkflowCreateParams(BaseModel):
    name: str
    description: str = ""
    definition: dict = Field(default_factory=dict)
    is_published: bool = False

@router.post("/new",
             summary="Create Workflow")
async def new(params: WorkflowCreateParams, session: DbSession, auth: Authenticated) -> int:
    """
    Creates a new workflow with the provided parameters.
    """

    workflow = Workflow(
        name=params.name,
        owner_id=auth.id,
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
    owner: User | None
    description: str
    definition: dict
    is_published: bool

@router.get("/{workflow_id}",
            summary="Show Single Workflow")
async def show(workflow: ExistingWorkflow, auth: Authenticated, session: DbSession) -> WorkflowShown:
    """
    Displays information for a single workflow.
    """

    if not can_access_workflow(auth, workflow, session):
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN)
    
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
async def edit(params: WorkflowUpdateParams, workflow: ExistingWorkflow, auth: Authenticated, session: DbSession) -> None:
    """
    Edits the attributes of a workflow.
    """

    if not can_access_workflow(auth, workflow, session, for_writing=True):
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN)

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
async def delete(workflow: ExistingWorkflow, auth: Authenticated, session: DbSession) -> None:
    """
    Deletes a workflow.
    """

    if not can_access_workflow(auth, workflow, session, for_writing=True):
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN)

    session.delete(workflow)


@router.post("/{workflow_id}/share/add",
             summary="Share Workflow")
async def add_share(write: bool, workflow: ExistingWorkflow, user: ExistingUser, auth: Authenticated, session: DbSession) -> None:
    """
    Gives read/write rights to a user, or changes the user's current rights.
    """

    if not can_access_workflow(auth, workflow, session, for_writing=True):
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN)

    share = get_workflow_share(user, workflow, session)

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
async def remove_share(workflow: ExistingWorkflow, user: ExistingUser, auth: Authenticated, session: DbSession) -> None:
    """
    Revokes the right of a user to read from / write to this workflow.
    """

    if not can_access_workflow(auth, workflow, session, for_writing=True):
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN)
    
    share = get_workflow_share(user, workflow, session)

    if share is not None:
        session.delete(share)