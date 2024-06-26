# std imports
import logging
from multiprocessing import Process
import socket

# 3rd party imports
from fastapi import FastAPI, Request
from pydantic_settings import BaseSettings
import uvicorn

# local imports
from nf_cloud_worker.web.nf_cloud_web_api_client import NFCloudWebApiClient

class WeblogProxySettings(BaseSettings):
    """
    Shared settings for FastAPI process.
    """
    client: NFCloudWebApiClient

class WeblogProxy:
    """
    Proxy for sending weblog requests to the NFCloud API using credentials.
    """

    def __init__(self, client: NFCloudWebApiClient, log_level: int):
        settings = WeblogProxySettings(
            client=client
        )
        # Get a free port
        self.__port = 0
        with socket.socket() as sock:
            sock.bind(('', 0))
            self.__port = sock.getsockname()[1]

        # Start the FastAPI server in a separate process
        self.__process = Process(target=self.__class__.server_process, args=(settings, self.__port, log_level))
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

    @staticmethod
    def server_process(settings: WeblogProxySettings, port: int, log_level: int):
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
        app = FastAPI()

        @app.post('/projects/{project_id:int}')
        async def handle_weblog_request(project_id: int, request: Request):
            """
            Receives the weblogs from Nextflow and forwards them to the NFCloud API.

            Parameters
            ----------
            project_id : ints
                Project ID
            """
            log = await request.body()
            try:
                settings.client.post_weblog(project_id, log)
            # pylint: disable=broad-except
            except Exception as e:
                # Catch everything to prevent the FastAPI server from crashing
                logging.error("Error while sending weblog to NFCloud API: %s", e)

        uvicorn.run(app, host="127.0.0.1", port=port, reload=False, log_level=log_level, access_log=log_level==logging.DEBUG)
