# std imports
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

    def __init__(self, client: NFCloudWebApiClient):
        settings = WeblogProxySettings(
            client=client
        )
        # Get a free port 
        self.__socket = socket.socket()
        self.__socket.bind(('', 0))
        self.__port = self.__socket.getsockname()[1]

        # Start the FastAPI server in a separate process
        self.__process = Process(target=self.__class__.server_process, args=(settings, self.__port))
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
    def server_process(settings: WeblogProxySettings, port: int):
        """
        Starts a FastAPI server to proxy weblog requests to the NFCloud API.
        Should be used in a separate process.s

        Parameters
        ----------
        settings : WeblogProxySettings
            Settings containing the NFCloud API URL and credentials
        port : int
            Port for the FastAPI server
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
            settings.client.post_weblog(project_id, log)
        uvicorn.run(app, host="127.0.0.1", port=port, use_colors=False, reload=False, log_level=logging.ERROR, access_log=False)
