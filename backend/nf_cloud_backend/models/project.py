from pathlib import Path
from typing import TYPE_CHECKING

from sqlalchemy import JSON, Column, ForeignKey, Integer
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