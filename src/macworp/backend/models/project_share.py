from typing import TYPE_CHECKING
from sqlalchemy import Column, ForeignKey, Integer
from sqlmodel import Field, Relationship, SQLModel

if TYPE_CHECKING:
	from .user import User
	from .project import Project


class ProjectShare(SQLModel, table=True):
	user_id: int | None = Field(
		default=None,
		sa_column=Column(Integer, ForeignKey("user.id", ondelete="CASCADE"), primary_key=True)
	)

	project_id: int | None = Field(
		default=None,
		sa_column=Column(Integer, ForeignKey("project.id", ondelete="CASCADE"), primary_key=True)
	)

	write: bool

	user: "User" = Relationship(back_populates="project_shares")

	project: "Project" = Relationship(back_populates="shares")
