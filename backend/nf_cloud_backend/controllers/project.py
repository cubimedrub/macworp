"""
API Endpoints with the prefix `/project`.
"""
from pathlib import Path
from typing import List, Literal
from urllib.parse import unquote

from fastapi import APIRouter, HTTPException, Query, File, UploadFile
from pydantic import BaseModel
from sqlmodel import Field, Session, select
from typing_extensions import Buffer

from .depends import Authenticated, ExistingProject, ExistingUser, ExistingUsers, OptionallyAuthenticated
from .workflow import ensure_read_access as ensure_workflow_read_access
from ..database import DbSession
from ..models.project import Project
from ..models.project_share import ProjectShare
from ..models.user import User, UserRole
from ..models.workflow import Workflow

# ---------------------------------------------------------
# HELPERS
# ---------------------------------------------------------


router = APIRouter(
    prefix="/project"
)

def add_file(self, target_file_path: Path, file: Buffer) -> Path:
    """
    Add file to directory
    """
    target_directory = self.get_path(target_file_path.parent)
    if not target_directory.is_dir():
        target_directory.mkdir(parents=True, exist_ok=True)
    with target_directory.joinpath(target_file_path.name).open(
        "wb"
    ) as project_file:
        project_file.write(file)

        return target_directory



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
async def list(auth: OptionallyAuthenticated, session: DbSession) -> list[int]:
    """
    Lists the IDs of all projects visible to this user. Requires authentication.
    """

    return [
        i.id
        for i in session.exec(select(Project)).all()
        if i.id is not None and can_access_project(auth, i, False, session)
    ]
@router.get("/count",
            summary="Count Projects")
async def count(auth: Authenticated, session: DbSession) -> int:
    """
    Returns numbers of Project
    """
    return sum(
        1
        for i in session.exec(select(Project)).all()
        if i.id is not None and can_access_project(auth, i, False, session)
    )


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
    read_shared: List[int]
    write_shared: List[int]

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
        is_published=project.is_published,
        read_shared=[share.user_id for share in project.shares if share.user_id is not None and not share.write],
        write_shared=[share.user_id for share in project.shares if share.user_id is not None and share.write]
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
        await add_share(True, project, [project.owner], auth, session)

    project.owner = user


@router.post("/{project_id}/delete",
             summary="Delete Project")
async def delete(project: ExistingProject, auth: Authenticated, session: DbSession) -> None:
    """
    Deletes a project. Requires ownership or admin rights.
    """

    ensure_owner(auth, project)

    session.delete(project)


@router.get("/{project_id}/files")
def list_files(
    project: ExistingProject,
    auth: Authenticated,
    directory_path: str = Query("/", description="Directory path", alias="dir")
):
    """
    List files
    """
    try:
        ensure_owner(auth, project)

        directory = project.get_path(Path(unquote(directory_path)))

        if not project.in_file_directory(directory):
            raise HTTPException(
                status_code=403,
                detail={"errors": {"general": "not in filedirectory"}}
            )

        if not directory.exists() or not directory.is_dir():
            raise HTTPException(
                status_code=404,
                detail={"errors": {"general": "directory not found"}}
            )

        files = []
        folders = []

        for entry in directory.iterdir():
            if entry.is_dir():
                folders.append(entry.name)
            elif entry.is_file():
                files.append(entry.name)

        folders.sort()
        files.sort()
        return {"folders": folders, "files": files}

    except PermissionError:
        raise HTTPException(
            status_code=403,
            detail={"errors": {"general": "access denied"}}
        )
    except OSError as e:
        raise HTTPException(
            status_code=500,
            detail={"errors": {"general": f"file system error: {str(e)}"}}
        )
    except Exception as e:
        raise HTTPException(
            status_code=500,
            detail={"errors": {"general": "internal server error: {str(e)}"}}
        )


@router.post('/{project_id}/upload-file',
             summary="Upload files to the project directory")
async def upload_file(project: ExistingProject,
                      auth: Authenticated,
                      session: DbSession,
                      file: UploadFile = File(..., description="File to upload"),
                      directory_path: str = Query("/", description="Directory path")):

    ensure_write_access(auth, project, session)

    if not file.filename or file.filename.strip() == "":
        raise HTTPException(
            status_code=400,
            detail="Invalid filename"
        )

    safe_filename = file.filename.replace('/', '_').replace('\\', '_').replace('..', '_')

    try:
        target_directory = project.get_path(Path(unquote(directory_path)))

        if not project.in_file_directory(target_directory):
            raise HTTPException(
                status_code=403,
                detail="Directory path is outside project directory"
            )

        target_path = target_directory / safe_filename

        if not project.in_file_directory(target_path):
            raise HTTPException(
                status_code=403,
                detail="Target file path is outside project directory"
            )

        file_content = await file.read()

        if len(file_content) > 10 * 1024 * 1024:
            raise HTTPException(
                status_code=413,
                detail="File too large (max 10MB)"
            )

        with target_path.open("wb") as f:
            f.write(file_content)

        return {
            "message": "File uploaded successfully",
            "filename": safe_filename,
            "path": str(target_path.relative_to(project.get_base_directory()))
        }

    except PermissionError:
        raise HTTPException(
            status_code=403,
            detail="Permission denied"
        )
    except OSError as e:
        raise HTTPException(
            status_code=500,
            detail=f"File system error: {str(e)}"
        )
    except Exception as e:
        raise HTTPException(
            status_code=500,
            detail="Internal server error"
        )


@router.post("/{project_id}/share/add",
             summary="Share Project")
async def add_share(write: bool, project: ExistingProject, users: ExistingUsers, auth: Authenticated, session: DbSession) -> None:
    """
    Gives read/write rights to some users, or changes the users' current rights. Requires write access.
    """

    ensure_write_access(auth, project, session)

    for user in users:
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
async def remove_share(project: ExistingProject, users: ExistingUsers, auth: Authenticated, session: DbSession) -> None:
    """
    Revokes the right of selected users to read from / write to this project. Requires write access.

    Note that ownership and admin rights override shared rights.
    """

    ensure_write_access(auth, project, session)

    for user in users:
        share = get_project_share(user, project, session)

        if share is not None:
            session.delete(share)