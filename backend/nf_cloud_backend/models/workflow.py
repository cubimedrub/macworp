from sqlalchemy import JSON, Column
from sqlmodel import Field, Relationship, SQLModel

class Workflow(SQLModel, table=True):
	id: int | None = Field(default=None, primary_key=True)

	"""
	A value of None means this workflow is "orphaned", i. e. the owner got deleted. 
	"""
	owner_id: int | None = Field(default=None, foreign_key="user.id", nullable=True)

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

	# shared_with: list["User"] = Relationship(back_populates="user", link_model="WorkflowShare")