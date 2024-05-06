
from typing import Annotated
from fastapi import Depends, HTTPException, status

from ..database import DbSession
from ..models.user import User
from ..models.workflow import Workflow


async def get_workflow(workflow_id: int, session: DbSession) -> Workflow:
    workflow = session.get(Workflow, workflow_id)
    if workflow is None:
        raise HTTPException(status_code=status.HTTP_422_UNPROCESSABLE_ENTITY, detail="Workflow not found")
    return workflow

ExistingWorkflow = Annotated[Workflow, Depends(get_workflow)]


async def get_user(user_id: int, session: DbSession) -> User:
    user = session.get(User, user_id)
    if user is None:
        raise HTTPException(status_code=status.HTTP_422_UNPROCESSABLE_ENTITY, detail="User not found")
    return user

ExistingUser = Annotated[Workflow, Depends(get_user)]