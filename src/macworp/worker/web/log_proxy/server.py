"""FastAPI server for proxying weblog requests to the MAcWorP API."""

import logging
from multiprocessing import Process
import socket

from fastapi import APIRouter, FastAPI
import uvicorn

from macworp.worker.web.backend_web_api_client import BackendWebApiClient
from macworp.worker.web.log_proxy.controllers.nextflow_controller import (
    NextflowController,
)
from macworp.configuration import Configuration
from macworp.worker.web.log_proxy.controllers.depends import get_backend_web_api_client
from macworp.worker.web.log_proxy.controllers.snakemake_controller import (
    SnakemakeController,
)


class Server:
    """
    Proxy for sending weblog requests to the MAcWorP API using credentials.
    """

    def __init__(
        self,
        config: Configuration,
        log_level: int,
    ):
        # Get a free port
        self.port = 0
        with socket.socket() as sock:
            sock.bind(("", 0))
            self.port = sock.getsockname()[1]

        # Start the FastAPI server in a separate process
        self.process = Process(
            target=self.__class__.server_process,
            args=(
                config,
                log_level,
                self.port,
            ),
        )
        try:
            self.process.start()
        except Exception as e:
            print("Error starting workflow logging server:", e)

    def __del__(self):
        if self.process is not None and self.process.is_alive():
            self.process.terminate()

    @classmethod
    def get_router(cls) -> APIRouter:
        """Bulds the FastAPI router with all paths for the weblog proxy."""

        router = APIRouter()
        router.add_api_route(
            "/nextflow/projects/{project_id:int}",
            NextflowController.weblogs,
            methods=["POST"],
        )

        router.add_api_route(
            "/snakemake/api/service-info",
            SnakemakeController.service_info,
            methods=["GET"],
        )

        router.add_api_route(
            "/snakemake/create_workflow",
            SnakemakeController.create_workflow,
            methods=["GET"],
        )

        router.add_api_route(
            "/snakemake/update_workflow_status",
            SnakemakeController.update_workflow_status,
            methods=["POST"],
        )

        router.add_api_route(
            "/snakemake/api/workflow/{project_id:int}",
            SnakemakeController.workflow,
            methods=["PUT"],
        )

        return router

    @classmethod
    def server_process(
        cls,
        config: Configuration,
        log_level: int,
        port: int,
    ):
        """
        Starts a FastAPI server to proxy weblog requests to the MAcWorP API.
        Should be used in a separate process.s

        Parameters
        ----------
        config : Configuration
            Configuration object with MAcWorP API URL and worker credentials
        port : int
            Port for the FastAPI server
        log_level : int
            Log level as defined in logging module
            If log level is DEBUG, access log will be enabled
        """

        async def get_configured_backend_web_api_client():
            return BackendWebApiClient(
                config.worker.macworp_url,
                config.worker.worker_credentials.username,
                config.worker.worker_credentials.password,
                config.worker.skip_cert_verification,
            )

        app = FastAPI()
        app.include_router(cls.get_router())

        # Because the settings coming via CLI and getting passed through the executor etc.
        # passing the setting as described in the FastAPI documentation is not possible.
        # Every path that uses the settings need an argument
        # `settings: Settings = Depends(get_dummy_settings)`
        # which is ultimately overridden by the `get_settings` function.
        app.dependency_overrides[get_backend_web_api_client] = (
            get_configured_backend_web_api_client
        )

        uvicorn.run(
            app,
            host="127.0.0.1",
            port=port,
            reload=False,
            log_level=log_level,
            access_log=log_level == logging.DEBUG,
        )
