"""
API Endpoints with the prefix `/project`.
"""

from typing import Literal
from fastapi import APIRouter, HTTPException, status
from pydantic import BaseModel
from sqlmodel import Field, Session, select

from ..database import DbSession

from .depends import Authenticated, ExistingProject, ExistingUser

from ..models.project import Project
from ..models.project_share import ProjectShare
from ..models.user import User, UserRole
from ..models.workflow import Workflow

from .workflow import ensure_read_access as ensure_workflow_read_access


# ---------------------------------------------------------
# HELPERS
# ---------------------------------------------------------


router = APIRouter(
    prefix="/project"
)


def get_project_share(user: User, project: Project, session: Session) -> ProjectShare | None:
    """
    Gets the ProjectShare identified by the provided `user` and `project`,
    or None if this project is not shared with the user.
    """

    return session.exec(
        select(ProjectShare).where(ProjectShare.user_id == user.id, ProjectShare.project_id == project.id)
    ).one_or_none() # [0]


def can_access_project(user: User, project: Project, for_writing: bool, session: Session) -> bool:
    """
    Helper method for common logic between `ensure_read_access` and `ensure_write_access`.
    """
    
    match user.role:
        case UserRole.admin:
            return True
        case UserRole.default:
            if project.owner_id == user.id or (not for_writing and project.is_published):
                return True
            share = get_project_share(user, project, session)
            return share is not None and (not for_writing or share.write)
    return False


def ensure_read_access(user: User, project: Project, session: Session) -> None:
    """
    Throws a `HTTPException` if `user` doesn't have the right to read from `project`.
    """

    if not can_access_project(user, project, False, session):
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="User not permitted read access to this project"
        )


def ensure_write_access(user: User, project: Project, session: Session) -> None:
    """
    Throws a `HTTPException` if `user` doesn't have the right to write to `project`.
    """
    
    if not can_access_project(user, project, True, session):
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="User not permitted write access to this project"
        )


def ensure_owner(user: User, project: Project) -> None:
    """
    Throws a `HTTPException` if `user` doesn't have the right to delete or transfer ownership of `project`.
    """

    if user.role != UserRole.admin and project.owner_id != user.id:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="User must be admin or owner of this project to perform operation"
        )


# ---------------------------------------------------------
# ENDPOINTS
# ---------------------------------------------------------


@router.get("/",
            summary="List Projects")
async def list(auth: Authenticated, session: DbSession) -> list[int]:
	"""
    Lists the IDs of all projects visible to this user. Requires authentication.
    """

	return [
        i.id
        for i in session.exec(select(Project)).all()
        if i.id is not None and can_access_project(auth, i, False, session)
    ]


class ProjectCreateParams(BaseModel):
    name: str
    workflow_id: int | None = None
    workflow_arguments: dict = Field(default_factory=dict)
    description: str = ""
    is_published: bool = False

@router.post("/new",
             summary="Create Project")
async def new(params: ProjectCreateParams, auth: Authenticated, session: DbSession) -> int:
    """
    Creates a new project with the provided parameters. Requires authentication.
    The authenticated user will be made the project's owner.
    """

    project = Project(
        name=params.name,
        owner=auth,
        workflow_id=params.workflow_id,
        workflow_arguments=params.workflow_arguments,
        description=params.description,
        is_published=params.is_published
    )
    session.add(project)
    session.commit()
    # response.status_code = status.HTTP_201_CREATED

    # Can't be None since commit will make the DB assign an ID
    assert project.id is not None

    return project.id


class ProjectShown(BaseModel):
    name: str
    owner_id: int | None
    workflow_id: int | None
    workflow_arguments: dict
    description: str
    is_published: bool

@router.get("/{project_id}",
            summary="Show Single Project")
async def show(project: ExistingProject, auth: Authenticated, session: DbSession) -> ProjectShown:
    """
    Displays information for a single project. Requires read access.
    """

    ensure_read_access(auth, project, session)

    return ProjectShown(
        name=project.name,
        owner_id=project.owner_id,
		workflow_id=project.workflow_id,
		workflow_arguments=project.workflow_arguments,
		description=project.description,
		is_published=project.is_published
    )


class ProjectUpdateParams(BaseModel):
    name: str | None = None
    workflow_id: int | Literal["unset"] | None = None
    workflow_arguments: dict | None = None
    description: str | None = None
    is_published: bool | None = None

@router.post("/{project_id}/edit",
             summary="Edit Project")
async def edit(params: ProjectUpdateParams, project: ExistingProject, auth: Authenticated, session: DbSession) -> None:
    """
    Edits the attributes of a project. Requires write access.

    If setting the workflow used, this additionally requires read access to the workflow.
    Unset the workflow by passing the literal string "unset" instead of an ID.
    """

    ensure_write_access(auth, project, session)

    if params.name is not None:
        project.name = params.name
    if params.workflow_id is not None:
        if params.workflow_id == "unset":
            project.workflow_id = None
        else:
            workflow = session.get(Workflow, params.workflow_id)
            if workflow is None:
                raise HTTPException(
                    status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
                    detail="Trying to link workflow that doesn't exist"
                )
            ensure_workflow_read_access(auth, workflow, session)
            project.workflow_id = params.workflow_id
    if params.workflow_arguments is not None:
        project.workflow_arguments = params.workflow_arguments
    if params.description is not None:
        project.description = params.description
    if params.is_published is not None:
        project.is_published = params.is_published


@router.post("/{project_id}/transfer_ownership",
             summary="Transfer Ownership")
async def transfer_ownership(project: ExistingProject, user: ExistingUser, auth: Authenticated, session: DbSession) -> None:
    """
    Makes another user the project owner. Requires ownership or admin rights.
    The former owner will be given write access.
    """

    ensure_owner(auth, project)

    if project.owner is not None:
        await add_share(True, project, project.owner, auth, session)

    project.owner = user


@router.post("/{project_id}/delete",
             summary="Delete Project")
async def delete(project: ExistingProject, auth: Authenticated, session: DbSession) -> None:
    """
    Deletes a project. Requires ownership or admin rights.
    """

    ensure_owner(auth, project)

    session.delete(project)


@router.post("/{project_id}/share/add",
             summary="Share Project")
async def add_share(write: bool, project: ExistingProject, user: ExistingUser, auth: Authenticated, session: DbSession) -> None:
    """
    Gives read/write rights to a user, or changes the user's current rights. Requires write access.
    """

    ensure_write_access(auth, project, session)

    share = get_project_share(user, project, session)

    if share is None:
        session.add(ProjectShare(
            user_id=user.id,
            project_id=project.id,
            write=write
        ))
    else:
        share.write = write


@router.post("/{project_id}/share/remove",
             summary="Un-Share Project")
async def remove_share(project: ExistingProject, user: ExistingUser, auth: Authenticated, session: DbSession) -> None:
    """
    Revokes the right of a user to read from / write to this project. Requires write access.

    Note that ownership and admin rights override shared rights.
    """

    ensure_write_access(auth, project, session)

    share = get_project_share(user, project, session)

    if share is not None:
        session.delete(share)