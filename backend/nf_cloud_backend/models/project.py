import json
import logging
import shutil
from enum import unique, Enum
from pathlib import Path
from typing import TYPE_CHECKING, IO, List, Dict, Any, Optional

import pika
from pydantic import BaseModel
from sqlalchemy import JSON, Column, ForeignKey, Integer
from sqlmodel import Field, Relationship, SQLModel, Session

from .supportedWorkflowEngine import SupportedWorkflowEngine
from .user import User
from .workflow import Workflow
from ..configuration import Configuration
from ..utils.src.macworp_utils.path import secure_joinpath

if TYPE_CHECKING:
    from .project_share import ProjectShare


@unique
class LogProcessingResultType(Enum):
    """Type of log which was processed"""

    PROGRESS = 1
    MESSAGE = 2
    ERROR = 3
    NONE = 4


class ProjectSchedulingError(Exception):
    """Custom exception for project scheduling errors"""


pass


class LogProcessingResult(BaseModel):
    """Result of processing a log entry"""

    type: LogProcessingResultType
    """True if processing log was not an error"""

    message: str
    """Log message"""


class Project(SQLModel, table=True):
    id: int | None = Field(default=None, primary_key=True)

    """
    A value of None means this project is "orphaned", i. e. the owner got deleted. 
    """
    owner_id: int | None = Field(
        default=None,
        sa_column=Column(Integer, ForeignKey("user.id", ondelete="SET NULL"), nullable=True)
    )
    owner: User | None = Relationship(back_populates="owned_projects")

    name: str = Field(max_length=512)

    """
    A description in Markdown format.
    """
    description: str = ""

    workflow_id: int | None = Field(default=None, foreign_key="workflow.id")
    workflow: Workflow | None = Relationship(back_populates="dependent_projects")

    workflow_arguments: dict = Field(default_factory=dict, sa_column=Column(JSON))

    """
    Published projects can be viewed by everyone.
    """
    is_published: bool = False

    shares: list["ProjectShare"] = Relationship(back_populates="project",
                                                sa_relationship_kwargs={"cascade": "all, delete-orphan"})

    """
    Ignored Projects will be ignored by the API.
    """
    ignore: bool = Field(default=False, nullable=False)

    """
    submission status
    """
    submitted_processes: int = Field(default=0, nullable=False)
    completed_processes: int = Field(default=0, nullable=False)

    is_scheduled: bool = Field(default=False, nullable=False)

    def get_base_directory(self) -> Path:
        """Get the base directory for this project's files"""
        base_path = Path.cwd() / "projects" / str(self.id)
        base_path.mkdir(parents=True, exist_ok=True)
        return base_path

    def get_path(self, relative_path: Path) -> Path:
        """Get absolute path for a relative path within the project"""
        base_dir = self.get_base_directory()

        if str(relative_path) == "/" or str(relative_path) == ".":
            return base_dir

        path_str = str(relative_path).lstrip("/")
        if not path_str:
            return base_dir

        return base_dir / path_str

    def in_file_directory(self, path: Path) -> bool:
        """Check if the given path is within the project's file directory"""
        try:
            base_dir = self.get_base_directory().resolve()
            target_path = path.resolve()

            try:
                target_path.relative_to(base_dir)
                return True
            except ValueError:
                return False

        except Exception:
            return False

    def is_ignored(self) -> bool:
        """Check if this project is ignored"""
        return self.ignore

    def finish(self) -> bool:
        """Finish the project submission"""
        self.is_scheduled = False
        self.submitted_processes = 0
        self.completed_processes = 0
        return True

    def create_folder(self, new_folder_path: Path) -> bool:
        """Create a new folder in the new work directory"""
        new_folder_path = self.get_path(new_folder_path)
        if not new_folder_path.is_dir():
            new_folder_path.mkdir(parents=True, exist_ok=True)
            return True
        return False

    def remove_path(self, folder_path) -> bool:
        """
        Deletes Path - handles dict, str, Path objects

        Args:
            folder_path: Can be:
                - dict: {'path': '/some/path'}
                - str: '/some/path' or 'relative/path'
                - Path: Path('/some/path') or Path('relative/path')
        """
        try:
            if isinstance(folder_path, dict):
                if 'path' in folder_path:
                    path_str = folder_path['path']
                    print(f"DEBUG: Extracted path from dict: {path_str}")
                else:
                    print("ERROR: Dictionary missing 'path' key")
                    return False
            elif isinstance(folder_path, (str, Path)):
                path_str = str(folder_path)
                print(f"DEBUG: Using direct path: {path_str}")
            else:
                print(f"ERROR: Unsupported path type: {type(folder_path)}")
                return False
            input_path = Path(path_str)
            print(f"DEBUG: Input path object: {input_path}")

        except Exception as e:
            print(f"ERROR: Failed to parse input path: {e}")
            return False

        if input_path.is_absolute():
            base_dir = self.get_base_directory()

            try:
                input_path.relative_to(base_dir.parent)
                full_path = input_path
                print(f"DEBUG: Using absolute path: {full_path}")
            except ValueError:
                print(f"WARNING: Absolute path {input_path} is outside project area")
                print(f"         Project base: {base_dir}")

                return False

        else:
            full_path = self.get_path(input_path)
            print(f"DEBUG: Resolved relative path to: {full_path}")


        if not full_path.exists():
            print("ERROR: Path does not exist!")
            return False

        try:
            if full_path.is_file():
                print("Deleting file...")
                full_path.unlink()
                print("SUCCESS: File deleted")
                return True

            elif full_path.is_dir():
                print("Deleting directory...")
                shutil.rmtree(full_path)
                print("SUCCESS: Directory deleted")
                return True

            else:
                print("ERROR: Path is neither file nor directory")
                return False

        except PermissionError as e:
            print(f"ERROR: Permission denied: {e}")
            return False
        except Exception as e:
            print(f"ERROR: Failed to delete: {e}")
            return False

    def get_file_size(self, file_path: Path) -> int:
        """Get the size of a file in the project"""
        file_path = self.get_path(file_path)
        return file_path.stat().st_size

    def get_metadata(self, file_path: Path) -> dict:
        """Returns the metadata of a file"""
        file_path = self.get_path(file_path)
        metadata_file_path = file_path.with_suffix(
            f"{file_path.suffix}.mmdata"
        )
        return json.load(metadata_file_path.open("r"))

    def get_cached_workflow_parameters(self, workflow_id: int) -> dict:
        """ returns the cached workflow parameters for this project """
        workflow_parameters_file = secure_joinpath(self.get_history_directory(), f"{workflow_id}.json")
        if workflow_parameters_file.is_file():
            return json.load(workflow_parameters_file.open('r'))
        return {"error": "Project or workflow not found"}

    def get_history_directory(self) -> Path:
        """Returns the history dir for MAcWorP specific files"""
        history_dir = secure_joinpath(self.get_cache_directory(), "history")
        if not history_dir.is_dir():
            history_dir.mkdir(parents=True, exist_ok=True)
        return history_dir

    def get_cache_directory(self) -> Path:
        """Returns the cache dir for MAcWorP specific files"""
        cache_dir = self.get_path(Path(".macworp_cache"))
        if not cache_dir.is_dir():
            cache_dir.mkdir(parents=True, exist_ok=True)
        return cache_dir

    def get_last_executed_cache_file(self) -> dict:
        return json.loads(secure_joinpath(self.get_cache_directory(), "last_executed_workflow.json").read_text())

    def get_last_executed_cache_file_path(self) -> Path:
        return secure_joinpath(self.get_cache_directory(), "last_executed_workflow.json")

    def add_file_chunk(self, target_file_path: Path, chunk_offset: int, file_chunk: IO[bytes]) -> Path:
        target_directory = self.get_path(target_file_path.parent)
        if not target_directory.is_dir():
            target_directory.mkdir(parents=True, exist_ok=True)

        full_file_path = target_directory / target_file_path.name

        with full_file_path.open("ab") as project_file:
            project_file.seek(chunk_offset)
            project_file.write(file_chunk.read())

        return target_file_path

    async def schedule_for_execution(
        self,
        session: Session,
        workflow,
        workflow_parameters: List[Dict[str, Any]]
    ) -> bool:
        """
        Schedule this project for execution.
        """
        from .queued_project import QueuedProject

        # Save cache files
        self.save_workflow_params_cache(workflow, workflow_parameters)
        self.save_last_executed_workflow_cache(workflow)

        # Create queued project
        queued_project = QueuedProject(
            id=self.id,
            workflow_id=workflow.id,
            workflow_arguments=workflow_parameters,
        )
        # Database transaction with RabbitMQ
        try:
            self.is_scheduled = True
            session.add(self)

            try:
                await self.publish_to_rabbitmq(queued_project)
            except Exception as e:
                logging.error(f"Failed to publish to RabbitMQ: {e}")
                raise ProjectSchedulingError("Failed to schedule project while publishing to QM") from e
        except ProjectSchedulingError:
            raise
        except Exception as e:
            logging.error(f"Database transaction failed: {e}")
            raise ProjectSchedulingError("Failed to schedule project") from e

        return self.is_scheduled

    def save_workflow_params_cache(self, workflow, workflow_parameters: List[Dict[str, Any]]):
        """
        Save workflow parameters to cache file
        """
        params_cache_file_path = workflow.get_workflow_params_cache_file()
        params_cache_data = {
            param["name"]: param["value"]
            for param in workflow_parameters
            if param.get("type") != "separator"
        }
        params_cache_file_path.write_text(json.dumps(params_cache_data))

    def save_last_executed_workflow_cache(self, workflow):
        """
        Save last executed workflow to cache file
        """
        last_executed_workflow_cache_file_path = self.get_last_executed_cache_file_path()
        last_executed_workflow_cache_file_path.write_text(
            json.dumps({"id": workflow.id})
        )

    def publish_to_rabbitmq(self, queued_project):
        """
        Publish project to RabbitMQ queue
        """
        connection = pika.BlockingConnection(
            pika.URLParameters(Configuration.values()["rabbit_mq"]["url"])
        )
        channel = connection.channel()
        channel.basic_publish(
            exchange="",
            routing_key=Configuration.values()["rabbit_mq"]["project_workflow_queue"],
            body=queued_project.model_dump_json().encode(),
        )
        connection.close()

    def process_workflow_log(
        self, log: Dict[str, Any], workflow_engine: SupportedWorkflowEngine
    ) -> LogProcessingResult:
        """
        Processes the workflow log and sends it to the web log proxy.

        Parameters
        ----------
        log : Dict[str, Any]
            Log
        workflow_engine : SupportedWorkflowEngine
            Workflow engine
        """
        match workflow_engine:
            case SupportedWorkflowEngine.NEXTFLOW:
                return self.process_nextflow_log(log)
            case SupportedWorkflowEngine.SNAKEMAKE:
                return self.process_snakemake_log(log)
        return LogProcessingResult(type=LogProcessingResultType.NONE, message="")

    def process_nextflow_log(self, log: Dict[str, Any]) -> LogProcessingResult:
        """
        Processes the Nextflow log and sends it to the web log proxy.

        Parameters
        ----------
        log : Dict[str, Any]
            Log
        """
        if "trace" in log and "event" in log:
            match log["event"]:
                case "process_submitted":
                    self.submitted_processes += 1  # type: ignore[assignment]
                case "process_completed":
                    self.completed_processes += 1  # type: ignore[assignment]
            self.save()
            return LogProcessingResult(
                type=LogProcessingResultType.PROGRESS,
                message=(
                    f"Task {log['trace']['task_id']}: "
                    f"{log['trace']['name']} - {log['trace']['status']}"
                ),
            )
        elif "metadata" in log and "event" in log:
            error_report: Optional[str] = log["metadata"]["workflow"].get(
                "errorReport", None
            )
            if error_report is not None:
                return LogProcessingResult(
                    type=LogProcessingResultType.ERROR, message=error_report
                )
        return LogProcessingResult(type=LogProcessingResultType.NONE, message="")

    def process_snakemake_log(self, log: Dict[str, Any]) -> LogProcessingResult:
        """
        Processes the Nextflow log and sends it to the web log proxy.

        Parameters
        ----------
        log : Dict[str, Any]
            Log
        """
        if "level" in log:
            match log["level"]:
                case "progress":
                    self.submitted_processes += log["done"]  # type: ignore[assignment]
                    self.completed_processes += log["total"]  # type: ignore[assignment]
                    self.save()
                    return LogProcessingResult(
                        type=LogProcessingResultType.PROGRESS,
                        message="",
                    )
                case "job_info":
                    msg = f"- {log['msg']}" if log["msg"] is not None else ""
                    return LogProcessingResult(
                        type=LogProcessingResultType.MESSAGE,
                        message=(f"Task {log['jobid']}: " f"{log['name']}{msg}"),
                    )
                case "run_info" | "info":
                    return LogProcessingResult(
                        type=LogProcessingResultType.MESSAGE,
                        message=log["msg"],
                    )
                case "error":
                    return LogProcessingResult(
                        type=LogProcessingResultType.ERROR,
                        message=log["msg"],
                    )
        return LogProcessingResult(type=LogProcessingResultType.NONE, message="")
