import enum
from typing import TYPE_CHECKING
from sqlmodel import Enum, Field, Relationship, SQLModel

if TYPE_CHECKING:
    from .workflow import Workflow
    from .workflow_share import WorkflowShare
    from .project import Project
    from .project_share import ProjectShare


class UserRole(str, enum.Enum):
    default = "default"
    admin = "admin"

# Note: "user" has special meaning in Postgres so when using psql it needs to be double-quoted!
class User(SQLModel, table=True):
    id: int | None = Field(default=None, primary_key=True)
    role: UserRole = Enum(UserRole)
    provider_type: str
    provider_name: str

    owned_workflows: list["Workflow"] = Relationship(back_populates="owner")
    owned_projects: list["Project"] = Relationship(back_populates="owner")

    workflow_shares: list["WorkflowShare"] = Relationship(back_populates="user")
    project_shares: list["ProjectShare"] = Relationship(back_populates="user")