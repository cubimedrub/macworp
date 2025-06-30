import json
from pathlib import Path
from typing import TYPE_CHECKING

from sqlalchemy import JSON, Column, ForeignKey, Integer, Boolean
from sqlmodel import Field, Relationship, SQLModel

from .user import User
from .workflow import Workflow

if TYPE_CHECKING:
    from .project_share import ProjectShare


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

    shares: list["ProjectShare"] = Relationship(back_populates="project")

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