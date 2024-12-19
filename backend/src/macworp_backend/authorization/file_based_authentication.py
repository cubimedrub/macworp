# std imports
import datetime
from typing import ClassVar, Dict, Any, Optional

# 3rd party imports
from flask import Request, jsonify, redirect, Response, url_for
from flask_login import login_user
from macworp_backend.utility.configuration import Configuration
import requests

# internal imports
from macworp_backend import file_auth_databases, app
from macworp_backend.authorization.abstract_authentication import AbstractAuthentication
from macworp_backend.authorization.provider_type import ProviderType
from macworp_backend.authorization.jwt import JWT
from macworp_backend.models.user import User


class FileBasedAuthentication(AbstractAuthentication):
    """File based authentication method."""

    PROVIDER_TYPE: ClassVar[ProviderType] = ProviderType.FILE

    FRONTEND_FILE_BASED_LOGIN_FORM: ClassVar[str] = "login/file-based"
    """Path to login form for file based authentication.
    """

    @classmethod
    def login(cls, request: Request, provider: str) -> Response:

        callback_url: str = url_for(
            "user_auth_callback",
            _external=True,
            _scheme=request.scheme,
            provider_type=cls.PROVIDER_TYPE.value,
            provider=provider,
        )

        if Configuration.values()["frontend_host_url"] is None:
            return redirect(
                f"{request.host_url}{cls.FRONTEND_FILE_BASED_LOGIN_FORM}?callback={callback_url}"
            )
        else:
            return redirect(
                f"{Configuration.values()['frontend_host_url']}/{cls.FRONTEND_FILE_BASED_LOGIN_FORM}?callback={callback_url}"
            )

    @classmethod
    def callback(cls, request: Request, provider: str) -> Response:
        """
        Handles the callback from the provider.

        Parameters
        ----------
        request : Request
            Provider callback request
        provider : str
            Provider name

        Returns
        -------
        Response
            Returns a json with the JWT token.
        """

        provider_client_config = cls.get_provider_client_config(provider)
        if provider_client_config is None:
            return jsonify({"errors": {"general": "Provider not supported."}}), 400

        data: Dict[str, Any] = request.get_json()

        # Get auth DB
        auth_db: Optional[Dict[str, Dict[str, str]]] = file_auth_databases.get(
            provider, None
        )
        if auth_db is None:
            return jsonify({"errors": {"general": "Provider not supported."}}), 400

        # Check login_id and password
        user_data: Dict[str, str] = {}
        try:
            user_data = auth_db.get(data["login_id"], {})
            if not user_data["password"] == data["password"]:
                raise ValueError()
        except (KeyError, ValueError) as err:
            app.logger.error(
                f"Error while authenticating user: {err}"
            )  # pylint: disable=no-member
            # Do not give any information which was wrong incase it was a brute force effort
            return jsonify({"errors": {"general": "Login ID or password wrong."}}), 400

        user = (
            User.select()
            .where(
                User.provider_type == cls.PROVIDER_TYPE.value,
                User.provider == provider,
                User.login_id == data["login_id"],
            )
            .get_or_none()
        )
        if user is None:
            user = User.create(
                provider_type=cls.PROVIDER_TYPE.value,
                provider=provider,
                login_id=data["login_id"],
                email=data["login_id"],
            )
        user.save()

        login_user(user)

        token = JWT.create_auth_token(
            app.config["SECRET_KEY"],
            user,
            provider_client_config["expires_in"]
            + int(datetime.datetime.utcnow().timestamp()),
        )

        return jsonify({"token": token})

    @classmethod
    def logout(cls, user: User) -> Response:
        return (
            jsonify(
                {
                    "errors": {
                        "general": "Logout is currently not supported for file based authentication."
                    }
                }
            ),
            501,
        )
