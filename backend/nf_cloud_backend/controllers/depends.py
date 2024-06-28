"""
This module contains FastAPI dependencies used by multiple controllers.
"""

from typing import Annotated
from fastapi import Depends, Header, HTTPException, status

from ..auth.jwt import JWT
from ..configuration import SECRET_KEY
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


async def get_users(user_ids: list[int], session: DbSession) -> list[User]:
    result = []
    for user_id in user_ids:
        result.append(await get_user(user_id, session))
    return result

ExistingUsers = Annotated[list[User], Depends(get_users)]
"""
Retrieves multiple users via the `user_ids` URL parameter. Throws an HTTPException if any of those users doesn't exist.
"""

async def get_optionally_authenticated_user(session: DbSession, x_token: Annotated[str | None, Header()] = None) -> User | None:
    if x_token is None:
        return None

    # TODO put actual authentication here
    try:
        user, expired = JWT.decode_auth_token_to_user(SECRET_KEY, x_token, session)
    except ValueError:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_ENTITY, detail="Token invalid")
    
    if expired:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_ENTITY, detail="Token expired")
    
    return user

OptionallyAuthenticated = Annotated[User | None, Depends(get_optionally_authenticated_user)]
"""
Returns the authenticated user, or None if no authentication is present.
Throws an HTTPException if authentication was attempted, but failed.
"""


async def get_authenticated_user(maybe_auth: OptionallyAuthenticated) -> User:
    if maybe_auth is None:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED)
    return maybe_auth

Authenticated = Annotated[User, Depends(get_authenticated_user)]
"""
Returns the authenticated user. Throws an HTTPException if no or incorrect authentication was provided.
"""