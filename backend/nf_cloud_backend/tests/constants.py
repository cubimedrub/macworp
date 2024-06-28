USER_1_ADMIN = 1
USER_2_DEFAULT = 2
USER_3_DB_AUTH = 3


class UserLogin:
    login_id: str
    password: str

    def __init__(self, login_id: str, password: str):
        self.login_id = login_id
        self.password = password

DEFAULT_LOGIN = UserLogin("devuser", "developer")
ADMIN_LOGIN = UserLogin("devadmin", "developer")


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