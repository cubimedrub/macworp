# 3rd party imports
from typing import Any, ClassVar, Dict, Optional
from flask import Request, redirect, Response
from macworp_backend.authorization.provider_type import ProviderType

# internal imports
from macworp_backend.models.user import User
from macworp_backend.utility.configuration import Configuration


class AbstractAuthentication:
    """Defines methods for handling the authentication process.
    1. Login - Redirects to the provider's login page
    2. callback - Provider redirect back to callback URL which returns a JWT token
    3. logout - Logs the user out
    """

    PROVIDER_TYPE: ClassVar[ProviderType] = None
    """
    Defines the provider type for config lookup.
    """

    @classmethod
    def login(cls, request: Request, provider: str) -> Response:
        """
        Logs the user in or redirects to the provider's login page.

        Parameters
        ----------
        request : Request
            Login request
        provider : str
            Provider name as defined in the config

        Returns
        -------
        Response
            Redirects to the providers login page.
        """
        raise NotImplementedError("Not implemented.")

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
            The response will contain the JWT token, where it is stored: body, header, ..., depends on the provider type.
        """
        raise NotImplementedError("Not implemented.")

    @classmethod
    def logout(cls, user: User) -> Response:
        """
        Logs the user out.

        Parameters
        ----------
        user : User
            User to logout
        """
        raise NotImplementedError("Not implemented.")

    @classmethod
    def redirect_to_frontend_callback(cls, request: Request, token: str) -> Response:
        """
        Redirects to frontend callback with given token.

        Parameters
        ----------
        request : Request
            Request
        token : str
            JWT token

        Returns
        -------
        Response
            Redirect to frontend root URL with JWT token as query parameter
        """
        if Configuration.values()["frontend_host_url"] is None:
            return redirect(f"{request.host_url}login/callback?token={token}")
        else:
            return redirect(
                f"{Configuration.values()['frontend_host_url']}/login/callback?token={token}"
            )

    @classmethod
    def get_provider_client_config(cls, provider: str) -> Optional[Dict[str, Any]]:
        """
        Returns config for the classes provider type and provider

        Parameters
        ----------
        provider : str
            Name of provider as given in the config file.

        Returns
        -------
        Optional[Dict[str, Any]]
            None if config was not found of a dictionary
        """
        return Configuration.values()["login_providers"][cls.PROVIDER_TYPE.value].get(
            provider, None
        )
