from typing import Any, Dict

from fastapi.responses import RedirectResponse
from fastapi import FastAPI
from oauthlib.oauth2 import WebApplicationClient
import requests
from sqlmodel import Session

from macworp.configuration import Configuration, OpenIdProviderConfiguration
from macworp.backend.auth.abstract_authorization import AbstractAuthorization
from macworp.backend.auth.login_request import LoginRequest

from macworp.backend.models.user import User
from macworp.backend.auth.provider_type import ProviderType


class OpenIDAuthorization(AbstractAuthorization):
    @classmethod
    def get_autodiscovery(
        cls, provider_client_config: OpenIdProviderConfiguration
    ) -> Dict[Any, Any]:
        """
        Get the autodiscovery.

        Parameters
        ----------
        provider_config : OpenIdProviderConfiguration
            Provider specific config from application config

        Returns
        -------
        Dict[Any, Any]
            Provider config
        """
        provider_info = requests.get(
            provider_client_config.backend.discovery_url,
            verify=provider_client_config.backend.verify_ssl,
        )
        return provider_info.json()

    @classmethod
    def login(
        cls,
        app: FastAPI,
        provider_name: str,
        login_request: LoginRequest,
        session: Session,
        config: Configuration,
    ) -> User:
        raise RuntimeError("Login works differently in OpenIDConnect")

    @classmethod
    def get_redirect(
        cls,
        app: FastAPI,
        provider_name: str,
        login_request: LoginRequest,
        session: Session,
        config: Configuration,
    ) -> RedirectResponse:
        provider_client_config = config.backend.login_providers.openid.get(
            provider_name, None
        )

        if provider_client_config is None:
            raise ValueError("Provider not supported.")

        print(f"PROVIDER CONFIG: {provider_client_config}")

        provider_config = cls.get_autodiscovery(provider_client_config)

        if provider_config is None:
            raise ValueError("Autodiscovery failure")

        provider_client = WebApplicationClient(provider_client_config.backend.client_id)

        redirect_uri: str = app.url_path_for(
            "user_auth_callback",
            # LoginRequest,
            # _external = True,
            # _scheme=login_request.schema,
            provider_type=ProviderType.OPENID_CONNECT.value,
            provider=provider_name,
        )

        # Use library to construct the request for Google login and provide
        # scopes that let you retrieve user's profile from Google
        request_uri = provider_client.prepare_request_uri(
            provider_config["authorization_endpoint"],
            redirect_uri=redirect_uri,
            scope=[provider_client_config.backend.get("scope", "openid"), "email"],
        )

        return RedirectResponse(request_uri)
