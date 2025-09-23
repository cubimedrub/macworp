from typing import TYPE_CHECKING
from sqlalchemy import Column, ForeignKey, Integer
from sqlmodel import Field, Relationship, SQLModel

if TYPE_CHECKING:
	from .user import User
	from .workflow import Workflow


class WorkflowShare(SQLModel, table=True):
	user_id: int | None = Field(
		default=None,
		sa_column=Column(Integer, ForeignKey("user.id", ondelete="CASCADE"), primary_key=True)
	)

	workflow_id: int | None = Field(
		default=None,
		sa_column=Column(Integer, ForeignKey("workflow.id", ondelete="CASCADE"), primary_key=True)
	)

	write: bool

	user: "User" = Relationship(back_populates="workflow_shares")

	workflow: "Workflow" = Relationship(back_populates="shares")
