"""
This module contains FastAPI dependencies used by multiple controllers.
"""

from typing import Annotated
from fastapi import Depends, HTTPException, status

from ..database import DbSession
from ..models.project import Project
from ..models.user import User
from ..models.workflow import Workflow


async def get_workflow(workflow_id: int, session: DbSession) -> Workflow:
    workflow = session.get(Workflow, workflow_id)
    if workflow is None:
        raise HTTPException(status_code=status.HTTP_422_UNPROCESSABLE_ENTITY, detail="Workflow not found")
    return workflow

ExistingWorkflow = Annotated[Workflow, Depends(get_workflow)]
"""
Retrieves a workflow via the `workflow_id` URL parameter. Throws an HTTPException if the workflow doesn't exist.
"""


async def get_project(project_id: int, session: DbSession) -> Project:
    project = session.get(Project, project_id)
    if project is None:
        raise HTTPException(status_code=status.HTTP_422_UNPROCESSABLE_ENTITY, detail="Project not found")
    return project

ExistingProject = Annotated[Project, Depends(get_project)]
"""
Retrieves a project via the `project_id` URL parameter. Throws an HTTPException if the project doesn't exist.
"""


async def get_user(user_id: int, session: DbSession) -> User:
    user = session.get(User, user_id)
    if user is None:
        raise HTTPException(status_code=status.HTTP_422_UNPROCESSABLE_ENTITY, detail="User not found")
    return user

ExistingUser = Annotated[User, Depends(get_user)]
"""
Retrieves a user via the `user_id` URL parameter. Throws an HTTPException if the user doesn't exist.
"""

async def get_optionally_authenticated_user(session: DbSession) -> User | None:
    # TODO put actual authentication here
    return session.get(User, 1)

OptionallyAuthenticated = Annotated[User | None, Depends(get_optionally_authenticated_user)]
"""
Returns the authenticated user, or None if no authentication is present.
Throws an HTTPException if authentication was attempted, but failed.
"""


async def get_authenticated_user(session: DbSession, maybe_auth: OptionallyAuthenticated) -> User:
    if maybe_auth is None:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED)
    return maybe_auth

Authenticated = Annotated[User, Depends(get_authenticated_user)]
"""
Returns the authenticated user. Throws an HTTPException if no or incorrect authentication was provided.
"""