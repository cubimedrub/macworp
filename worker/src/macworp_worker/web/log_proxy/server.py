"""FastAPI server for proxying weblog requests to the MAcWorP API."""

import logging
from multiprocessing import Process
import socket

from fastapi import APIRouter, FastAPI
import uvicorn

from macworp_worker.web.backend_web_api_client import BackendWebApiClient
from macworp_worker.web.log_proxy.settings import Settings, get_dummy_settings
from macworp_worker.web.log_proxy.controllers.nextflow_controller import (
    NextflowController,
)


class Server:
    """
    Proxy for sending weblog requests to the MAcWorP API using credentials.
    """

    def __init__(self, client: BackendWebApiClient, log_level: int):
        settings = Settings(client=client)
        # Get a free port
        self.__port = 0
        with socket.socket() as sock:
            sock.bind(("", 0))
            self.__port = sock.getsockname()[1]

        # Start the FastAPI server in a separate process
        self.__process = Process(
            target=self.__class__.server_process,
            args=(settings, self.__port, log_level),
        )
        self.__process.start()

    def __del__(self):
        if self.__process is not None and self.__process.is_alive():
            self.__process.terminate()

    @property
    def port(self) -> int:
        """
        Returns the port the proxy is running on.

        Returns
        -------
        int
            Port
        """
        return self.__port

    @classmethod
    def get_router(cls) -> APIRouter:
        """Bulds the FastAPI router with all paths for the weblog proxy."""

        router = APIRouter()
        router.add_api_route(
            "/nextflow/projects/{project_id:int}",
            NextflowController.weblogs,
            methods=["POST"],
        )

        return router

    @classmethod
    def server_process(cls, settings: Settings, port: int, log_level: int):
        """
        Starts a FastAPI server to proxy weblog requests to the MAcWorP API.
        Should be used in a separate process.s

        Parameters
        ----------
        settings : WeblogProxySettings
            Settings containing the MAcWorP API URL and credentials
        port : int
            Port for the FastAPI server
        log_level : int
            Log level as defined in logging module
            If log level is DEBUG, access log will be enabled
        """

        async def get_settings():
            return settings

        app = FastAPI()
        app.include_router(cls.get_router())

        # Because the settings coming via CLI and getting passed through the executor etc.
        # passing the setting as described in the FastAPI documentation is not possible.
        # Every path that uses the settings need an argument
        # `settings: Settings = Depends(get_dummy_settings)`
        # which is ultimately overridden by the `get_settings` function.
        app.dependency_overrides[get_dummy_settings] = get_settings

        uvicorn.run(
            app,
            host="127.0.0.1",
            port=port,
            reload=False,
            log_level=log_level,
            access_log=log_level == logging.DEBUG,
        )
