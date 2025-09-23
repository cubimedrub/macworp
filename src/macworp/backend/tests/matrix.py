from .user_login import UserLogin


class MatrixRole:
    """
    Objects of this class are column in the "permission matrix".
    """
    
    name: str
    """
    Human-readable name (used to name tests)
    """

    login: UserLogin | None
    """
    User credentials, or None if no authentication should be performed
    """
    
    workflow: int
    """
    A workflow ID (doesn't make sense for all routes)
    """

    project: int
    """
    A project ID (doesn't make sense for all routes) 
    """

    def __init__(self, name: str, login: UserLogin | None, workflow: int, project: int):
        self.name = name
        self.login = login
        self.workflow = workflow
        self.project = project


def name_func(test_func, _, param):
    """
    Naming function for the parameterization decorator
    """
    
    return f"{test_func.__name__}_{param.args[0].name}"