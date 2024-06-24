from .abstract_authorization import AbstractAuthorization
from .login_request import LoginRequest
from sqlmodel import Session, select
from ..models.user import User
from nf_cloud_backend import openid_clients
from .provider_type import ProviderType

from typing import Any, ClassVar, Dict
from fastapi import Request
from fastapi.responses import RedirectResponse
import requests

class OpenIDAuthorization(AbstractAuthorization):
    @classmethod
    def get_autodiscovery(cls, provider_client_config: Dict[str, Any]) -> Dict[Any, Any]:
        """
        Get the autodiscovery.

        Parameters
        ----------
        provider_config : Dict[str, Any]
            Provider specific config from application config

        Returns
        -------
        Dict[Any, Any]
            Provider config
        """
        return requests.get(
            provider_client_config["discovery_url"],
            verify=provider_client_config.get("verify_ssl", True)
        ).json()
    @classmethod
    def login(cls, provider_name: str, login_request: LoginRequest, session: Session) -> User:
        
        provider_client_config = cls.get_provider_client_config(provider_name)

        if provider_client_config is None:
            raise ValueError(f"Provider not supported.")


        provider_config = cls.get_autodiscovery(provider_client_config)
        provider_client = openid_clients[provider_name]

        redirect_uri: str = Request.url_for(
            "user_auth_callback",
            _external = True,
            _scheme=login_request.scheme,
            provider_type = ProviderType.OPENID_CONNECT.value,
            provider = provider_name
        )
        
        # Use library to construct the request for Google login and provide
        # scopes that let you retrieve user's profile from Google
        request_uri = provider_client.prepare_request_uri(
            provider_config["authorization_endpoint"],
            redirect_uri = redirect_uri,
            scope = [
                provider_client_config.get("scope", "openid"),
                "email"
            ]
        )
        return RedirectResponse(request_uri)
