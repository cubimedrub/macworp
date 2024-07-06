"""
This module contains human-readable names for entries in the testing database (see db_seed.yaml)
"""

from .user_login import UserLogin
from .matrix import MatrixRole


DEFAULT_LOGIN = UserLogin("devuser", "developer")
ADMIN_LOGIN = UserLogin("devadmin", "developer")


USER_1_ADMIN = 1
USER_2_DEFAULT = 2
USER_3_DB_AUTH = 3


WORKFLOW_1_OWNED = 1
WORKFLOW_2_PRIVATE = 2
WORKFLOW_3_READ_SHARED = 3
WORKFLOW_4_WRITE_SHARED = 4
WORKFLOW_5_PUBLIC = 5


PROJECT_1_OWNED = 1
PROJECT_2_PRIVATE = 2
PROJECT_3_READ_SHARED = 3
PROJECT_4_WRITE_SHARED = 4
PROJECT_5_PUBLIC = 5


UNAUTHENTICATED = MatrixRole(
    "unauthenticated",
    None,
    WORKFLOW_1_OWNED,
    PROJECT_1_OWNED
)

DEFAULT = MatrixRole(
    "default",
    DEFAULT_LOGIN,
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