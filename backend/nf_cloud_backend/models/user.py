import enum
from sqlmodel import Enum, Field, SQLModel

class UserRole(str, enum.Enum):
	default = "default"
	admin = "admin"

# Note: "user" has special meaning in Postgres so when using psql it needs to be double-quoted!
class User(SQLModel, table=True):
	id: int | None = Field(default=None, primary_key=True)
	role: UserRole = Enum(UserRole)
	provider_type: str
	provider_name: str