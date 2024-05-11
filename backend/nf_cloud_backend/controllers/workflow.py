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

from .depends import Authenticated, ExistingUser, ExistingWorkflow

from ..models.user import User, UserRole
from ..models.workflow import Workflow
from ..models.workflow_share import WorkflowShare

from ..database import DbSession


router = APIRouter(
    prefix="/workflow"
)


def get_workflow_share(user: User, workflow: Workflow) -> WorkflowShare | None:
    return Session.object_session(user).exec(
        select(WorkflowShare).where(WorkflowShare.user_id == user.id, WorkflowShare.workflow_id == workflow.id)
    ).one_or_none()[0]


def can_access_workflow(user: User, workflow: Workflow, for_writing: bool) -> bool:
    match user.role:
        case UserRole.admin:
            return True
        case UserRole.default:
            if workflow.owner_id == user.id:
                return True
            share = get_workflow_share(user, workflow)
            return share is not None and (not for_writing or share.write)


def ensure_read_access(user: User, workflow: Workflow) -> None:
    if not can_access_workflow(user, workflow, for_writing=False):
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="User not permitted read access to this workflow"
        )


def ensure_write_access(user: User, workflow: Workflow) -> None:
    if not can_access_workflow(user, workflow, for_writing=True):
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="User not permitted write access to this workflow"
        )


def ensure_owner(user: User, workflow: Workflow) -> None:
    if user.role != UserRole.admin and workflow.owner_id != user.id:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="User must be admin or owner of this workflow to perform operation"
        )


@router.get("/",
            summary="List Workflows")
async def list(auth: Authenticated) -> list[int]:
    """
    Lists the IDs of all workflows visible to this user.
    """
    
    return [
        i[0].id
        for i in Session.object_session(auth).exec(select(Workflow)).all()
        if can_access_workflow(auth, i[0], for_writing=False)
    ]


class WorkflowCreateParams(BaseModel):
    name: str
    description: str = ""
    definition: dict = Field(default_factory=dict)
    is_published: bool = False

@router.post("/new",
             summary="Create Workflow")
async def new(params: WorkflowCreateParams, auth: Authenticated) -> int:
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
    Session.object_session(auth).add(workflow)
    Session.object_session(auth).commit()
    # response.status_code = status.HTTP_201_CREATED
    return workflow.id


class WorkflowShown(BaseModel):
    name: str
    owner_id: int | None
    description: str
    definition: dict
    is_published: bool

@router.get("/{workflow_id}",
            summary="Show Single Workflow")
async def show(workflow: ExistingWorkflow, auth: Authenticated) -> WorkflowShown:
    """
    Displays information for a single workflow.
    """

    ensure_read_access(auth, workflow)

    return WorkflowShown(
        name=workflow.name,
        owner_id=workflow.owner_id,
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
async def edit(params: WorkflowUpdateParams, workflow: ExistingWorkflow, auth: Authenticated) -> None:
    """
    Edits the attributes of a workflow.
    """

    ensure_write_access(auth, workflow)

    if params.name is not None:
        workflow.name = params.name
    if params.description is not None:
        workflow.description = params.description
    if params.definition is not None:
        workflow.definition = params.definition
    if params.is_published is not None:
        workflow.is_published = params.is_published


@router.post("/{workflow_id}/transfer_ownership",
             summary="Transfer Ownership")
async def transfer_ownership(workflow: ExistingWorkflow, user: ExistingUser, auth: Authenticated) -> None:
    ensure_owner(auth, workflow)

    # Give write access to the former owner
    if workflow.owner is not None:
        await add_share(True, workflow, workflow.owner, auth)

    workflow.owner = user


@router.post("/{workflow_id}/delete",
             summary="Delete Workflow")
async def delete(workflow: ExistingWorkflow, auth: Authenticated) -> None:
    """
    Deletes a workflow.
    """

    ensure_owner(auth, workflow)

    Session.object_session(workflow).delete(workflow)


@router.post("/{workflow_id}/share/add",
             summary="Share Workflow")
async def add_share(write: bool, workflow: ExistingWorkflow, user: ExistingUser, auth: Authenticated) -> None:
    """
    Gives read/write rights to a user, or changes the user's current rights.
    """

    ensure_write_access(auth, workflow)

    share = get_workflow_share(user, workflow)

    if share is None:
        Session.object_session(auth).add(WorkflowShare(
            user_id=user.id,
            workflow_id=workflow.id,
            write=write
        ))
    else:
        share.write = write


@router.post("/{workflow_id}/share/remove",
             summary="Un-Share Workflow")
async def remove_share(workflow: ExistingWorkflow, user: ExistingUser, auth: Authenticated) -> None:
    """
    Revokes the right of a user to read from / write to this workflow.
    """

    ensure_write_access(auth, workflow)
    
    share = get_workflow_share(user, workflow)

    if share is not None:
        Session.object_session(share).delete(share)