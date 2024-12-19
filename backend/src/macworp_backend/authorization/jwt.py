# std imports
import datetime
from typing import ClassVar, Tuple

# 3rd party imports
import jwt

# internal imports
from macworp_backend.models.user import User


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
    def is_token_valid(cls, token_payload: dict) -> bool:
        """
        Checks if the token is still valid.

        Parameters
        ----------
        token_payload : dict
            Decoded token payload

        Returns
        -------
        bool
            True if valid.
        """

    @classmethod
    def decode_auth_token_to_user(
        cls, secret_key: str, auth_token: str
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
        return User.select().where(User.id == data["user_id"]).get_or_none(), data[
            "expires_at"
        ] > int(datetime.datetime.utcnow().timestamp())
