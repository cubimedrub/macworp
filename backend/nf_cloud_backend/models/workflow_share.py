from sqlmodel import Field, SQLModel


class WorkflowShare(SQLModel, table=True):
	user_id: int | None = Field(default=None, foreign_key="user.id", primary_key=True)
	workflow_id: int | None = Field(default=None, foreign_key="workflow.id", primary_key=True)