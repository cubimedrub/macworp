"""
API Endpoints with the prefix `/project`.
"""
from pathlib import Path
from typing import List, Literal, Any, Coroutine, Optional, Dict
from urllib.parse import unquote

import pandas as pd
from fastapi import APIRouter, HTTPException, Query, File, UploadFile, Form, Header
from pydantic import BaseModel, errors, json
from sqlmodel import Field, Session, select
from starlette.responses import JSONResponse, FileResponse
from typing_extensions import Buffer

from .depends import Authenticated, ExistingProject, ExistingUser, ExistingUsers, OptionallyAuthenticated, \
    ExistingWorkflow
from .workflow import ensure_read_access as ensure_workflow_read_access
from .. import connectionManager
from ..connectionManager import ConnectionManager
from ..database import DbSession
from ..models.project import Project, LogProcessingResultType, ProjectSchedulingError
from ..models.project_share import ProjectShare
from ..models.user import User, UserRole
from ..models.workflow import Workflow

from ..models.supportedWorkflowEngine import SupportedWorkflowEngine
import zipfile
import tempfile

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
    ).one_or_none()  # [0]


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


def file_download(path: Path, is_inline: bool) -> FileResponse:
    """
    Downloads a file inline or as attachment
    """
    return FileResponse(
        path=path,
        filename=path.name,
        media_type='application/octet-stream',
        headers={
            "Content-Disposition": f"{'inline' if is_inline else 'attachment'}; filename={path.name}"
        }
    )


def table_download(path: Path) -> JSONResponse:
    """
    Downloads table as JSON, created with Pandas `DataFrame.to_json(orient="split")`, e.g.
    """
    dataframe: Optional[pd.DataFrame] = None

    try:
        match path.suffix.lower():
            case ".csv":
                dataframe = pd.read_csv(path)
            case ".tsv":
                dataframe = pd.read_csv(path, sep="\t")
            case ".xlsx":
                dataframe = pd.read_excel(path)
    except pd.errors.EmptyDataError:
        dataframe = pd.DataFrame()

    if dataframe is None:
        raise errors
    # todo raise UnknownTableFormat(["CSV", "TSV", "XLSX"]) definieren oder umgehen

    json_data = dataframe.to_json(orient="split", index=False)
    return JSONResponse(
        content=json_data,
        media_type="application/json"
    )


def folder_download(path: Path, is_inline: bool) -> FileResponse:
    """
    Downloads a folder as ZIP file
    """
    with tempfile.NamedTemporaryFile(delete=False, suffix='.zip') as tmp_file:
        with zipfile.ZipFile(tmp_file.name, 'w', zipfile.ZIP_DEFLATED) as zip_file:
            # Add all files from directory to ZIP
            for file_path in path.rglob('*'):
                if file_path.is_file():
                    arc_name = file_path.relative_to(path)
                    zip_file.write(file_path, arc_name)

        return FileResponse(
            path=tmp_file.name,
            filename=f"{path.name}.zip",
            media_type='application/zip',
            headers={
                "Content-Disposition": f"{'inline' if is_inline else 'attachment'}; filename={path.name}.zip"
            }
        )


# ---------------------------------------------------------
# ENDPOINTS
# ---------------------------------------------------------


@router.get("/",
            summary="List Projects")
async def list(auth: Authenticated, session: DbSession,
               offset: int = Query(0, ge=0, description="Number of items to skip"),
               limit: int = Query(50, ge=1, le=1000, description="Number of items to return")
               ):
    """
     Parameters:
    - offset: Number of projects to skip (default: 0)
    - limit: Maximum number of projects to return (default: 50, max: 1000)

    Returns:
    - List of projects ordered by ID (descending) and then by name
    """
    query = (
        select(Project)
        .order_by(Project.id.desc(), Project.name)
        .offset(offset)
        .limit(limit)
    )

    projects = session.exec(query).all()

    return projects


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
async def transfer_ownership(project: ExistingProject, user: ExistingUser, auth: Authenticated,
                             session: DbSession) -> None:
    """
    Makes another user the project owner. Requires ownership or admin rights.
    The former owner will be given write access.
    """

    ensure_owner(auth, project)

    if project.owner is not None:
        await add_share(True, project, [project.owner], auth, session)

    project.owner = user


@router.delete("/{project_id}/delete",
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


@router.post("/{project_id}/upload-file-chunk",
             summary="Upload file chunks to the project directory. Useful for uploading large files.")
async def upload_file_chunk(project: ExistingProject, auth: Authenticated, session: DbSession,
                            is_dropzone: Optional[int] = Query(0, alias="is-dropzone"), file: UploadFile = File(...),
                            file_path: UploadFile = File(...), dzchunkbyteoffset: Optional[int] = Form(None),
                            chunk_offset: Optional[int] = Form(None, alias="chunk-offset")) -> JSONResponse:
    ensure_write_access(auth, project, session)

    if not file:
        errors["file"].append("cannot be empty")
    if not file_path:
        errors["file_path"].append("cannot be empty")

    file_path_content = await file_path.read()
    file_path_str = file_path_content.decode("utf-8")
    file_path_obj = Path(file_path_str)

    if is_dropzone and is_dropzone > 0:
        chunk_offset_value = dzchunkbyteoffset or 0
    else:
        chunk_offset_value = chunk_offset or 0

    result_file_path = project.add_file_chunk(
        file_path_obj,
        chunk_offset_value,
        file.file
    )

    return JSONResponse(
        content={"file_path": str(result_file_path)},
        status_code=200
    )


@router.post("/{project_id}/create-folder",
             summary="Create Folder")
async def create_folder(project: ExistingProject, auth: Authenticated, session: DbSession, path: Path) -> JSONResponse:
    ensure_write_access(auth, project, session)
    if not path:
        raise HTTPException(status_code=422, detail={"errors": {"request body": ["path cannot be empty"]}})
    project.create_folder(path)

    return JSONResponse(content="", status_code=200)


@router.post("/{project_id}/delete-path",
             summary="Deletes a path")
async def delete_path(project: ExistingProject, auth: Authenticated, session: DbSession, path: Path) -> JSONResponse:
    ensure_write_access(auth, project, session)
    if not path:
        raise HTTPException(status_code=422, detail={"errors": {"request body": ["path cannot be empty"]}})
    project.remove_path(path)
    return JSONResponse(content="", status_code=200)


@router.get("/{project_id}/is-ignored",
            summary="Check if Project is Ignored")
async def is_ignored(project: ExistingProject, auth: Authenticated, session: DbSession) -> bool:
    ensure_read_access(auth, project, session)
    return project.is_ignored()


@router.post("/{project_id}/finished",
             summary="Set Project as Finished")
async def is_finished(project: ExistingProject, auth: Authenticated, session: DbSession) -> None:
    ensure_write_access(auth, project, session)
    if project.finish():
        session.add(project)
        session.commit()
    else:
        raise HTTPException(status_code=500, detail="PFroject couldn't be finished")


@router.get("/{project_id}/file-size",
            summary="Get Project File Size")
async def get_file_size(project: ExistingProject, auth: Authenticated, session: DbSession,
                        file_path: Path = Query(..., description="File path")) -> int:
    ensure_read_access(auth, project, session)
    return project.get_file_size(file_path)


@router.get("/{project_id}/metadata",
            summary="Get Project Metadata of a File")
async def get_metadata(project: ExistingProject,
                       auth: Authenticated,
                       session: DbSession,
                       file_path: Path = Query(..., description="File path")
                       ) -> dict:
    ensure_read_access(auth, project, session)
    return project.get_metadata(file_path)


@router.get("/{project_id}/cached-workflow-parameters/{workflow_id}",
            summary="Returns the cached workflow parameters of a Project")
async def cached_workflow_parameters(project: ExistingProject, workflow_id: int, auth: Authenticated,
                                     session: DbSession) -> dict:
    ensure_read_access(auth, project, session)

    return project.get_cached_workflow_parameters(workflow_id)


@router.post("/{project_id}/schedule/{workflow_id}",
             summary="Endpoint to schedule project for execution in RabbitMQ")
async def schedule(project: ExistingProject, workflow: ExistingWorkflow, auth: Authenticated,
                   session: DbSession, workflow_parameters: List[Dict[str, Any]]) -> JSONResponse:
    # auth checkup
    ensure_write_access(auth, project, session)
    ensure_workflow_read_access(auth, workflow, session)

    # Project checks
    if project.ignore:
        raise HTTPException(
            status_code=409,
            detail={"errors": {"general": "project is ignored and cannot be scheduled"}}
        )

    if project.is_scheduled:
        raise HTTPException(
            status_code=422,
            detail={"errors": {"general": "project is already scheduled"}}
        )

    # Parameter validation
    workflow.validate_workflow_parameters(workflow_parameters)

    try:
        is_scheduled = await project.schedule_for_execution(session, workflow, workflow_parameters)
        return JSONResponse(status_code=200, content={"is_scheduled": is_scheduled})
    except ProjectSchedulingError as e:
        raise HTTPException(
            status_code=500,
            detail={"errors": {"general": str(e)}}
        )


@router.get("/{project_id}/last-executed-workflow",
            summary="Returns the last cached workflow parameters of a project")
async def last_cached_workflow_parameters(project: ExistingProject, auth: Authenticated, session: DbSession) -> dict:
    ensure_read_access(auth, project, session)
    return project.get_last_executed_cache_file()


WEBLOG_WORKFLOW_ENGINE_HEADER = "X-Workflow-Engine"


@router.post("/{project_id}/workflow-log",
             summary=" Endpoint for Nextflow to report log. If log is received, a event is send to the browser with submitted and completed processes.")
async def workflow_log(project: ExistingProject, auth: Authenticated, session: DbSession,
                       workflow_log: dict,
                       workflow_engine_header: str = Header(alias=WEBLOG_WORKFLOW_ENGINE_HEADER)) -> JSONResponse:
    if not workflow_log:
        raise HTTPException(status_code=422, detail={"errors": {"request body": ["cannot be empty"]}})

    engine = SupportedWorkflowEngine.from_str(workflow_engine_header)

    workflow = project.workflow
    if workflow is None:
        raise HTTPException(status_code=400, detail="Project has no associated workflow.")

    workflow.validate_engine(engine)

    log_processing_result = project.process_workflow_log(workflow_log, engine)

    manager = ConnectionManager()
    match log_processing_result.type:
        case LogProcessingResultType.PROGRESS | LogProcessingResultType.MESSAGE:
            print(f"WOULD EMIT: new-progress to project {project.id}")
            await manager.send_message(
                project.id,
                {
                    "event": "new-progress",
                    "submitted_processes": project.submitted_processes,
                    "completed_processes": project.completed_processes,
                    "details": log_processing_result.message,
                }
            )
        case LogProcessingResultType.ERROR:
            print(f"WOULD EMIT: error to project {project.id}")
            await manager.send_message(
                project.id,
                {
                    "event": "error",
                    "error_report": log_processing_result.message,
                }
            )

    return JSONResponse(content="", status_code=200)


@router.post("/{project_id}/share/add",
             summary="Share Project")
async def add_share(write: bool, project: ExistingProject, users: ExistingUsers, auth: Authenticated,
                    session: DbSession) -> None:
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


@router.get("/{project_id}/download",
            summary="Downloads a file or folder")
async def download(project: ExistingProject, auth: Authenticated, session: DbSession,
                   path: str = Query("", description="Path to download"),
                   is_inline: bool = Query(False, alias="is-inline", description="If true, inline download"),
                   with_metadata: bool = Query(False, alias="with-metadata", description="Include metadata headers"),
                   is_table: bool = Query(False, alias="is-table", description="Return table as JSON")):
    ensure_write_access(auth, project, session)

    decoded_path = Path(unquote(path))
    path_to_download = project.get_path(decoded_path)
    if path_to_download.is_file():
        response = None

        if not is_table:
            response = file_download(path_to_download, is_inline)
        else:
            try:
                response = table_download(path_to_download)
            except Exception as error:
                raise HTTPException(
                    status_code=422,
                    detail={
                        "errors": {
                            "general": f"unknown table format, supported types are: {', '.join(error.supported_table_formats)}",
                        }
                    }
                )

        # Add metadata headers if requested
        if with_metadata:
            metadata_file_path = path_to_download.with_suffix(
                f"{path_to_download.suffix}.mmdata"
            )
            metadata = {}
            if metadata_file_path.is_file():
                metadata = json.loads(metadata_file_path.read_text())

            response.headers["MMD-Header"] = metadata.get("header", "")
            response.headers["MMD-Description"] = metadata.get("description", "")

        return response

    elif path_to_download.is_dir():
        return folder_download(path_to_download, is_inline)
    else:
        raise HTTPException(
            status_code=404,
            detail={"errors": {"path": ["not found"]}}
        )
