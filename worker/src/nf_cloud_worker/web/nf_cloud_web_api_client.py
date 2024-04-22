# std imports
import json
from typing import Dict

# 3rd party imports
import requests
from requests.auth import HTTPBasicAuth

class NFCloudWebApiClient:
    """
    Client for communicating with the NFCloud API.
    """

    HEADERS: Dict[str, str] = {
        "Connection": "close"
    }
    """Default header for requests
    """

    TIMEOUT: int = 60
    """Timeout for connections
    """

    def __init__(self, nf_cloud_base_url: str, nf_cloud_api_user: str, nf_cloud_api_password: str):
        """
        Creates a new NFCloudWebApiClient.

        Parameters
        ----------
        nf_cloud_base_url : str
            Base URL of the NFCloud API
        nf_cloud_api_user : str
            Username for the NFCloud API
        nf_cloud_api_password : str
            Password for the NFCloud API
        """
        self.__nf_cloud_base_url = nf_cloud_base_url
        self.__nf_cloud_api_usr = nf_cloud_api_user
        self.__nf_cloud_api_pwd = nf_cloud_api_password

    def get_workflow(self, workflow_id: int):
        """
        Get a workflow by ID.

        Parameters
        ----------
        workflow_id : int
            Workflow ID

        Returns
        -------
        Dict[str, Any]
            Workflow
        
        Raises
        ------
        ValueError
            If the request was not successful.
        """
        with requests.get(
            f"{self.__nf_cloud_base_url}/api/workflows/{workflow_id}",
            headers=self.__class__.HEADERS,
            timeout=self.__class__.TIMEOUT

        ) as response:
            if not response.ok:
                raise ValueError(f"Error posting finish: {response.text}")
            
            workflow = response.json()

            return workflow


    def get_weblog_url(self, project_id: int) -> str:
        """
        Returns the web log url for a given project id.

        Parameters
        ----------
        project_id : int
            Project ID

        Returns
        -------
        str
            Web log url
        """
        weblog_url: str = f"{self.__nf_cloud_base_url}/api/projects/{project_id}/workflow-log"
        return weblog_url.replace("://", f"://{self.__nf_cloud_api_usr}:{self.__nf_cloud_api_pwd}@")

    def post_finish(self, project_id: int):
        """
        Marks a project run as finished.

        Parameters
        ----------
        project_id : int
            Project ID

        Raises
        ------
        ValueError
            If the request was not successful.
        """
        with requests.post(
            f"{self.__nf_cloud_base_url}/api/projects/{project_id}/finished",
            auth=HTTPBasicAuth(
                self.__nf_cloud_api_usr,
                self.__nf_cloud_api_pwd
            ),
            headers = self.__class__.HEADERS,
            timeout=self.__class__.TIMEOUT
        ) as response:
            if not response.ok:
                raise ValueError(f"Error posting finish: {response.text}")

