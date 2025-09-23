from pydantic import BaseModel


class LoginRequest(BaseModel):
    """
    Request body for the login endpoint.
    """

    login_id: str
    """Users login ID
    """
    password: str
    """Users password (plain)
    """
