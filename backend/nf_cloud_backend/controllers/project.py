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


from fastapi import APIRouter, HTTPException, status
from pydantic import BaseModel
from sqlmodel import Field, Session, select

from .depends import Authenticated, ExistingProject, ExistingUser

from ..models.project import Project
from ..models.project_share import ProjectShare
from ..models.user import User, UserRole


router = APIRouter(
    prefix="/project"
)


def get_project_share(user: User, project: Project) -> ProjectShare | None:
    return Session.object_session(user).exec(
        select(ProjectShare).where(ProjectShare.user_id == user.id, ProjectShare.project_id == project.id)
    ).one_or_none()[0]


def can_access_project(user: User, project: Project, for_writing: bool) -> bool:
    match user.role:
        case UserRole.admin:
            return True
        case UserRole.default:
            if project.owner_id == user.id:
                return True
            share = get_project_share(user, project)
            return share is not None and (not for_writing or share.write)


def ensure_read_access(user: User, project: Project) -> None:
    if not can_access_project(user, project, for_writing=False):
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="User not permitted read access to this project"
        )


def ensure_write_access(user: User, project: Project) -> None:
    if not can_access_project(user, project, for_writing=True):
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="User not permitted write access to this project"
        )


def ensure_owner(user: User, project: Project) -> None:
    if user.role != UserRole.admin and project.owner_id != user.id:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="User must be admin or owner of this project to perform operation"
        )


@router.get("/",
            summary="List Projects")
async def list(auth: Authenticated) -> list[int]:
	"""
    Lists the IDs of all projects visible to this user.
    """

	return [
        i[0].id
        for i in Session.object_session(auth).exec(select(Project)).all()
        if can_access_project(auth, i[0], for_writing=False)
    ]


class ProjectCreateParams(BaseModel):
    name: str
    workflow_id: int | None = None
    workflow_arguments: dict = Field(default_factory=dict)
    description: str = ""
    is_published: bool = False

@router.post("/new",
             summary="Create Project")
async def new(params: ProjectCreateParams, auth: Authenticated) -> int:
    """
    Creates a new project with the provided parameters.
    """

    project = Project(
        name=params.name,
        owner=auth,
        workflow_id=params.workflow_id,
        workflow_arguments=params.workflow_arguments,
        description=params.description,
        is_published=params.is_published
    )
    Session.object_session(auth).add(project)
    Session.object_session(auth).commit()
    # response.status_code = status.HTTP_201_CREATED
    return project.id


class ProjectShown(BaseModel):
    name: str
    workflow_id: int | None
    workflow_arguments: dict
    description: str
    is_published: bool

@router.get("/{project_id}",
            summary="Show Single Project")
async def show(project: ExistingProject, auth: Authenticated) -> ProjectShown:
    """
    Displays information for a single project.
    """

    ensure_read_access(auth, project)

    return ProjectShown(
        name=project.name,
		workflow_id=project.workflow_id,
		workflow_arguments=project.workflow_arguments,
		description=project.description,
		is_published=project.is_published
    )


class ProjectUpdateParams(BaseModel):
    name: str | None = None
    # Potential issue here -- no way to unset the workflow like this. I hate python.
    workflow_id: int | None = None
    workflow_arguments: dict | None = None
    description: str | None = None
    is_published: bool | None = None

@router.post("/{project_id}/edit",
             summary="Edit Project")
async def edit(params: ProjectUpdateParams, project: ExistingProject, auth: Authenticated) -> None:
    """
    Edits the attributes of a project.
    """

    ensure_write_access(auth, project)

    if params.name is not None:
        project.name = params.name
    if params.workflow_id is not None:
        project.workflow_id = params.workflow_id
    if params.workflow_arguments is not None:
        project.workflow_arguments = params.workflow_arguments
    if params.description is not None:
        project.description = params.description
    if params.is_published is not None:
        project.is_published = params.is_published


@router.post("/{project_id}/transfer_ownership",
             summary="Transfer Ownership")
async def transfer_ownership(project: ExistingProject, user: ExistingUser, auth: Authenticated) -> None:
    ensure_owner(auth, project)

    # Give write access to the former owner
    if project.owner is not None:
        await add_share(True, project, project.owner, auth)

    project.owner = user


@router.post("/{project_id}/delete",
             summary="Delete Project")
async def delete(project: ExistingProject, auth: Authenticated) -> None:
    """
    Deletes a project.
    """

    ensure_owner(auth, project)

    Session.object_session(project).delete(project)


@router.post("/{project_id}/share/add",
             summary="Share Project")
async def add_share(write: bool, project: ExistingProject, user: ExistingUser, auth: Authenticated) -> None:
    """
    Gives read/write rights to a user, or changes the user's current rights.
    """

    ensure_write_access(auth, project)

    share = get_project_share(user, project)

    if share is None:
        Session.object_session(auth).add(ProjectShare(
            user_id=user.id,
            project_id=project.id,
            write=write
        ))
    else:
        share.write = write


@router.post("/{project_id}/share/remove",
             summary="Un-Share Project")
async def remove_share(project: ExistingProject, user: ExistingUser, auth: Authenticated) -> None:
    """
    Revokes the right of a user to read from / write to this project.
    """

    ensure_write_access(auth, project)
    
    share = get_project_share(user, project)

    if share is not None:
        Session.object_session(share).delete(share)