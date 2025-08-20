from typing import Annotated

from fastapi import Depends

from macworp.worker.web.backend_web_api_client import (
    BackendWebApiClient as NonDependsBackendWebApiClient,
)


async def get_backend_web_api_client() -> NonDependsBackendWebApiClient:
    """Dummy function to return a backend web API client which is alter replaced
    when building the FastAPI app.

    Returns
    -------
    BackendWebApiClient
        A dummy client with empty URL and credentials
    """
    return NonDependsBackendWebApiClient("", "", "", False)


BackendWebApiClient = Annotated[
    NonDependsBackendWebApiClient, Depends(get_backend_web_api_client)
]
