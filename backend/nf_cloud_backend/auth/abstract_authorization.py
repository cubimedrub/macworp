from fastapi import FastAPI
from sqlmodel import Session
from typing import Any, ClassVar, Dict, Optional

from .provider_type import ProviderType

from .login_request import LoginRequest
from ..models.user import User
from nf_cloud_backend.configuration import Configuration

class AbstractAuthorization:
    @classmethod
    def login(cls, app: FastAPI, provider_name: str, login_request: LoginRequest, session: Session) -> User:
        raise NotImplementedError("method should be implemented in subclass")
    
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
        return Configuration.values()["login_providers"][ProviderType.OPENID_CONNECT.value].get(provider, None)
    
    
