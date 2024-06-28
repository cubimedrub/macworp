from parameterized import parameterized

from httpx import Request

from .base import DONT_CARE, Test
from .constants import *


class MatrixRole:
    name: str

    login: UserLogin | None
    
    workflow: int

    project: int


    def __init__(self, name: str, login: UserLogin | None, workflow: int, project: int):
        self.name = name
        self.login = login
        self.workflow = workflow
        self.project = project


UNAUTHENTICATED = MatrixRole(
    "unauthenticated",
    None,
    WORKFLOW_1_OWNED,
    PROJECT_1_OWNED
)

PRIVATE = MatrixRole(
    "private",
    DEFAULT_LOGIN,
    WORKFLOW_2_PRIVATE,
    PROJECT_2_PRIVATE
)

READ_ACCESS = MatrixRole(
    "read_access",
    DEFAULT_LOGIN,
    WORKFLOW_3_READ_SHARED,
    PROJECT_3_READ_SHARED
)

WRITE_ACCESS = MatrixRole(
    "write_access",
    DEFAULT_LOGIN,
    WORKFLOW_4_WRITE_SHARED,
    PROJECT_4_WRITE_SHARED
)

OWNED = MatrixRole(
    "owned",
    DEFAULT_LOGIN,
    WORKFLOW_1_OWNED,
    PROJECT_1_OWNED
)

ADMIN = MatrixRole(
    "admin",
    ADMIN_LOGIN,
    WORKFLOW_1_OWNED,
    PROJECT_1_OWNED
)


def name_func(test_func, param_num, param):
    return f"{test_func.__name__}_{param.args[0].name}"    