"""Settings for the log proxy"""

from pydantic_settings import BaseSettings

from macworp_worker.web.backend_web_api_client import BackendWebApiClient


class Settings(BaseSettings):
    """
    Shared settings for FastAPI process.
    """

    client: BackendWebApiClient
    """Client for communicating with the MAcWorP API"""


async def get_dummy_settings() -> Settings:
    """Returns settings with a dummy settings as default dependency for paths.

    Returns
    -------
    Settings
        Settings with a dummy client
    """
    return Settings(client=BackendWebApiClient("", "", "", True))
