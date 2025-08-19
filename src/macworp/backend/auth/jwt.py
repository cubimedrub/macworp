# std imports
import datetime
from typing import ClassVar, Tuple

# 3rd party imports
import jwt
from sqlmodel import select, Session

# internal imports
from ..models.user import User
from ..database import global_db_engine


class JWT:
    """
    Class to encode and decode JWT tokens
    """

    SIGNING_ALGORTIHTM: ClassVar[str] = "HS256"
    """JWT signing algorithm
    """

    @classmethod
    def create_auth_token(cls, secret_key: str, user: User, expires_at: int) -> str:
        """
        Creates an authentication token for the given user and provider

        Parameters
        ----------
        secret_key : str
            Secret key for encoding
        user : User
            User

        Returns
        -------
        str
            JWT token
        """
        return jwt.encode(
            {"user_id": user.id, "expires_at": expires_at},
            secret_key,
            algorithm=cls.SIGNING_ALGORTIHTM,
        )

    @classmethod
    def decode_auth_token(cls, secret_key: str, auth_token: str) -> dict:
        """
        Decoded the given JWT token from the given authentication header.

        Parameters
        ----------
        secret_key : str
            Secret key for encoding
        auth_header : str
            Header of format `JWT <jwt_token>`

        Returns
        -------
        dict
            Decoded payload
        """
        return jwt.decode(auth_token, secret_key, algorithms=[cls.SIGNING_ALGORTIHTM])

    @classmethod
    def decode_auth_token_to_user(
        cls, secret_key: str, auth_token: str, session: Session
    ) -> Tuple[User, bool]:
        """
        Decoded the given JWT token from the given authentication header.

        Parameters
        ----------
        secret_key : str
            Secret key for encoding
        auth_header : str
            Header of format `JWT <jwt_token>`

        Returns
        -------
        Tuple
            With user and if token was unexpired
        """
        data = cls.decode_auth_token(secret_key, auth_token)

        user = session.get(User, data["user_id"])
        if user is None:
            raise ValueError("User not found")
        return (user, data["expires_at"] > int(datetime.datetime.utcnow().timestamp()))
