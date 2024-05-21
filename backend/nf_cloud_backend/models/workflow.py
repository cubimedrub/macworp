from typing import TYPE_CHECKING
from sqlalchemy import JSON, Column
from sqlmodel import Field, Relationship, SQLModel, Session, select

from .user import User

if TYPE_CHECKING:
    from .project import Project
    from .workflow_share import WorkflowShare


class Workflow(SQLModel, table=True):
    id: int | None = Field(default=None, primary_key=True)

    """
    A value of None means this workflow is "orphaned", i. e. the owner got deleted. 
    """
    owner_id: int | None = Field(default=None, foreign_key="user.id", nullable=True)
    owner: User | None = Relationship(back_populates="owned_workflows")

    name: str = Field(max_length=512)

    """
    A description in Markdown format.
    """
    description: str = ""

    definition: dict = Field(default_factory=dict, sa_column=Column(JSON))

    """
    Published workflows can be viewed by everyone.
    """
    is_published: bool = False

    shares: list["WorkflowShare"] = Relationship(back_populates="workflow")

    dependent_projects: list["Project"] = Relationship(back_populates="workflow")