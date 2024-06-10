import enum
from typing import TYPE_CHECKING
from sqlmodel import Field, Relationship, SQLModel

if TYPE_CHECKING:
    from .workflow import Workflow
    from .workflow_share import WorkflowShare
    from .project import Project
    from .project_share import ProjectShare

class UserRole(str, enum.Enum):
    default = "default"
    admin = "admin"


    @classmethod
    def from_str(cls, role: str):
        """
        Converts a string to a UserRole enum.

        Parameters
        ----------
        role : str
            The role as a string

        Returns
        -------
        UserRole

        Raises
        ------
        ValueError
            If the role is invalid
        """

        match role:
            case "default":
                return cls.default
            case "admin":
                return cls.admin
            case _:
                raise ValueError(f"Invalid role: {role}")

class User(SQLModel, table=True):
    id: int | None = Field(default=None, primary_key=True)
    login_id: str | None = None
    role: UserRole
    provider_type: str
    provider_name: str
    email: str 
    hashed_password: str | None = None
    disabled: bool | None = None

    owned_workflows: list["Workflow"] = Relationship(back_populates="owner")
    owned_projects: list["Project"] = Relationship(back_populates="owner")

    workflow_shares: list["WorkflowShare"] = Relationship(back_populates="user")
    project_shares: list["ProjectShare"] = Relationship(back_populates="user")
