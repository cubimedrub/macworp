import logging
from typing import Any, Callable, Dict

from fastapi import APIRouter, Depends, FastAPI
import uvicorn


def get_default_settings() -> Dict[str, Any]:
    return {}


def test(settings: Dict[str, Any] = Depends(get_default_settings)):
    return settings


def get_router() -> APIRouter:
    router = APIRouter()
    router.add_api_route(
        "/",
        test,
        methods=["GET"],
    )

    return router


def main(settings: Dict[str, Any], port: int, log_level: int):
    """
    Starts a FastAPI server to proxy weblog requests to the NFCloud API.
    Should be used in a separate process.s

    Parameters
    ----------
    settings : WeblogProxySettings
        Settings containing the NFCloud API URL and credentials
    port : int
        Port for the FastAPI server
    log_level : int
        Log level as defined in logging module
        If log level is DEBUG, access log will be enabled
    """

    async def get_settings():
        return settings

    app = FastAPI()
    app.include_router(get_router())
    # app.dependency_overrides[get_default_settings] = get_settings

    uvicorn.run(
        app,
        host="127.0.0.1",
        port=port,
        reload=False,
        log_level=log_level,
        access_log=log_level == logging.DEBUG,
    )


if __name__ == "__main__":
    main({"msg": "Hello World"}, 9000, logging.DEBUG)
