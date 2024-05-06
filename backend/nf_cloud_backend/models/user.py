import enum
from typing import TYPE_CHECKING
from sqlmodel import Enum, Field, Relationship, SQLModel

if TYPE_CHECKING:
	from .workflow_share import WorkflowShare


class UserRole(str, enum.Enum):
	default = "default"
	admin = "admin"

# Note: "user" has special meaning in Postgres so when using psql it needs to be double-quoted!
class User(SQLModel, table=True):
	id: int | None = Field(default=None, primary_key=True)
	role: UserRole = Enum(UserRole)
	provider_type: str
	provider_name: str

	workflow_shares: list["WorkflowShare"] = Relationship(back_populates="user")