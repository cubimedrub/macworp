
from typing import TYPE_CHECKING
from sqlalchemy import JSON, Column
from sqlmodel import Field, Relationship, SQLModel

if TYPE_CHECKING:
	from .project_share import ProjectShare


class Project(SQLModel, table=True):
	id: int | None = Field(default=None, primary_key=True)

	"""
	A value of None means this project is "orphaned", i. e. the owner got deleted. 
	"""
	owner_id: int | None = Field(default=None, foreign_key="user.id", nullable=True)

	name: str = Field(max_length=512)

	workflow_id: int | None = Field(default=None, foreign_key="workflow.id")

	workflow_arguments: dict = Field(default_factory=dict, sa_column=Column(JSON))

	"""
	Published projects can be viewed by everyone.
	"""
	is_published: bool = False

	shares: list["ProjectShare"] = Relationship(back_populates="project")