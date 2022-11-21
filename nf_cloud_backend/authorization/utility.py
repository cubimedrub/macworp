# std imports
from typing import Optional, Any, Dict

# internal imports
from nf_cloud_backend.authorization.provider_type import ProviderType
from nf_cloud_backend.utility.configuration import Configuration

class Utility:
    """
    Utilities for authorization.
    """
    @classmethod
    def get_provider_client_config(cls, provider_type: ProviderType, provider: str) -> Optional[Dict[str, Any]]:
        """
        Returns config for the given provider_type and provider

        Parameters
        ----------
        provider_type : ProviderType
            Provider type
        provider : str
            Name of provider as given in the config file.

        Returns
        -------
        Optional[Dict[str, Any]]
            None if config was not found of a dictionary
        """
        return Configuration.values()["login_providers"][provider_type.value].get(provider, None)
