"""
API Endpoints with the prefix `/workflow`.
"""

from typing import Annotated, List
from fastapi import APIRouter, Body, Depends, HTTPException, Header, Response, status
from pydantic import BaseModel, Field
from sqlmodel import SQLModel, Session, select

from .depends import Authenticated, ExistingUser, ExistingUsers, ExistingWorkflow

from ..models.user import User, UserRole
from ..models.workflow import Workflow
from ..models.workflow_share import WorkflowShare

from ..database import DbSession

# ---------------------------------------------------------
# HELPERS
# ---------------------------------------------------------


router = APIRouter(
    prefix="/workflow"
)


def get_workflow_share(user: User, workflow: Workflow, session: Session) -> WorkflowShare | None:
    """
    Gets the WorkflowShare identified by the provided `user` and `workflow`,
    or None if this workflow is not shared with the user.
    """

    return session.exec(
        select(WorkflowShare).where(WorkflowShare.user_id == user.id, WorkflowShare.workflow_id == workflow.id)
    ).one_or_none()


def can_access_workflow(user: User, workflow: Workflow, for_writing: bool, session: Session) -> bool:
    """
    Helper method for common logic between `ensure_read_access` and `ensure_write_access`.
    """

    match user.role:
        case UserRole.admin:
            return True
        case UserRole.default:
            if workflow.owner_id == user.id or (not for_writing and workflow.is_published):
                return True
            share = get_workflow_share(user, workflow, session)
            return share is not None and (not for_writing or share.write)
    return False


def ensure_read_access(user: User, workflow: Workflow, session: Session) -> None:
    """
    Throws a `HTTPException` if `user` doesn't have the right to read from `workflow`.
    """

    if not can_access_workflow(user, workflow, False, session):
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="User not permitted read access to this workflow"
        )


def ensure_write_access(user: User, workflow: Workflow, session: Session) -> None:
    """
    Throws a `HTTPException` if `user` doesn't have the right to write to `workflow`.
    """

    if not can_access_workflow(user, workflow, True, session):
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="User not permitted write access to this workflow"
        )


def ensure_owner(user: User, workflow: Workflow) -> None:
    """
    Throws a `HTTPException` if `user` doesn't have the right to delete or transfer ownership of `workflow`.
    """

    if user.role != UserRole.admin and workflow.owner_id != user.id:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="User must be admin or owner of this workflow to perform operation"
        )


@router.get("/",
            summary="List Workflows")
async def list(auth: Authenticated, session: DbSession) -> list[int]:
    """
    Lists the Workflows
    """
    query = (
        select(Workflow)
        .order_by(Workflow.id.desc(), Workflow.name)
    )
    workflows = session.exec(query).all()
    return workflows


class WorkflowCreateParams(BaseModel):
    name: str
    description: str = ""
    definition: dict = Field(default_factory=dict)
    is_published: bool = False


@router.post("/new",
             summary="Create Workflow")
async def new(params: WorkflowCreateParams, auth: Authenticated, session: DbSession) -> int:
    """
    Creates a new workflow with the provided parameters. Requires authentication.
    The authenticated user will be made the workflow's owner.
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

    # Can't be None since commit will make the DB assign an ID
    assert workflow.id is not None

    return workflow.id


class WorkflowShown(BaseModel):
    name: str
    owner_id: int | None
    description: str
    definition: dict
    is_published: bool
    read_shared: List[int]
    write_shared: List[int]


@router.get("/{workflow_id}",
            summary="Show Single Workflow")
async def show(workflow: ExistingWorkflow, auth: Authenticated, session: DbSession) -> WorkflowShown:
    """
    Displays information for a single workflow. Requires read access.
    """

    ensure_read_access(auth, workflow, session)

    return WorkflowShown(
        name=workflow.name,
        owner_id=workflow.owner_id,
        description=workflow.description,
        definition=workflow.definition,
        is_published=workflow.is_published,
        read_shared=[share.user_id for share in workflow.shares if share.user_id is not None and not share.write],
        write_shared=[share.user_id for share in workflow.shares if share.user_id is not None and share.write]
    )


class WorkflowUpdateParams(BaseModel):
    name: str | None = None
    description: str | None = None
    definition: dict | None = None
    is_published: bool | None = None


@router.post("/{workflow_id}/edit",
             summary="Edit Workflow")
async def edit(params: WorkflowUpdateParams, workflow: ExistingWorkflow, auth: Authenticated,
               session: DbSession) -> None:
    """
    Edits the attributes of a workflow. Requires write access.
    """

    ensure_write_access(auth, workflow, session)

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
async def transfer_ownership(workflow: ExistingWorkflow, user: ExistingUser, auth: Authenticated,
                             session: DbSession) -> None:
    """
    Makes another user the workflow owner. Requires ownership or admin rights.
    The former owner will be given write access.
    """

    ensure_owner(auth, workflow)

    if workflow.owner is not None:
        await add_share(True, workflow, [workflow.owner], auth, session)

    workflow.owner = user


@router.post("/{workflow_id}/delete",
             summary="Delete Workflow")
async def delete(workflow: ExistingWorkflow, auth: Authenticated, session: DbSession) -> None:
    """
    Deletes a workflow. Requires ownership or admin rights.
    """

    ensure_owner(auth, workflow)

    session.delete(workflow)


@router.post("/{workflow_id}/share/add",
             summary="Share Workflow")
async def add_share(write: bool, workflow: ExistingWorkflow, users: ExistingUsers, auth: Authenticated,
                    session: DbSession) -> None:
    """
    Gives read/write rights to a user, or changes the user's current rights. Requires write access.
    """

    ensure_write_access(auth, workflow, session)

    for user in users:
        share = get_workflow_share(user, workflow, session)

        if share is None:
            session.add(WorkflowShare(
                user_id=user.id,
                workflow_id=workflow.id,
                write=write
            ))
        else:
            share.write = write


@router.post("/{workflow_id}/share/remove",
             summary="Un-Share Workflow")
async def remove_share(workflow: ExistingWorkflow, users: ExistingUsers, auth: Authenticated,
                       session: DbSession) -> None:
    """
    Revokes the right of a user to read from / write to this workflow. Requires write access.

    Note that ownership and admin rights override shared rights.
    """

    ensure_write_access(auth, workflow, session)

    for user in users:
        share = get_workflow_share(user, workflow, session)

        if share is not None:
            session.delete(share)
