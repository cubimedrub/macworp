# std imports
import json
import logging
from time import sleep
from typing import ClassVar, Dict

# 3rd party imports
import requests
from requests.auth import HTTPBasicAuth


class NFCloudWebApiClient:
    """
    Client for communicating with the NFCloud API.
    """

    HEADERS: Dict[str, str] = {"Connection": "close"}
    """Default header for requests
    """

    TIMEOUT: int = 60
    """Timeout for connections
    """

    API_CALL_TRIES: ClassVar[int] = 3
    """Number of tries for each API calls before giving up."""

    RETRY_TIMEOUT: ClassVar[int] = 3
    """Timeout between retries in seconds"""

    def __init__(
        self, nf_cloud_base_url: str, nf_cloud_api_user: str, nf_cloud_api_password: str
    ):
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
        url = f"{self.__nf_cloud_base_url}/api/workflows/{workflow_id}"
        for i in range(self.__class__.API_CALL_TRIES):
            try:
                with requests.get(
                    url,
                    headers=self.__class__.HEADERS,
                    timeout=self.__class__.TIMEOUT,
                ) as response:
                    if not response.ok:
                        raise ValueError(f"Error posting finish: {response.text}")

                    workflow = response.json()

                    return workflow
            except requests.exceptions.ConnectionError as e:
                if i < self.__class__.API_CALL_TRIES - 1:
                    logging.error(
                        "[WORKER / API CLIENT / ATTEMPT %i] Error while getting workflow from API: %s",
                        i + 1,
                        e,
                    )
                    sleep(self.__class__.RETRY_TIMEOUT)
                    continue

                raise e

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
        url = f"{self.__nf_cloud_base_url}/api/projects/{project_id}/finished"
        for i in range(self.__class__.API_CALL_TRIES):
            try:
                with requests.post(
                    url,
                    auth=HTTPBasicAuth(
                        self.__nf_cloud_api_usr, self.__nf_cloud_api_pwd
                    ),
                    headers=self.__class__.HEADERS,
                    timeout=self.__class__.TIMEOUT,
                ) as response:
                    if not response.ok:
                        raise ValueError(f"Error posting finish: {response.text}")
            except requests.exceptions.ConnectionError as e:
                if i < self.__class__.API_CALL_TRIES - 1:
                    logging.error(
                        "[WORKER / API CLIENT / ATTEMPT %i] Error while sending sending finish to API: %s",
                        i + 1,
                        e,
                    )
                    sleep(self.__class__.RETRY_TIMEOUT)
                    continue
                raise e

    def post_weblog(self, project_id: int, log: bytes):
        """
        Posts a web log entry.

        Parameters
        ----------
        project_id : int
            Project ID
        log : Dict[str, str]
            Log entry

        Raises
        ------
        ValueError
            If the request was not successful.
        """
        headers = self.__class__.HEADERS.copy()
        headers["Content-Type"] = "application/json"

        for i in range(self.__class__.API_CALL_TRIES):
            try:
                with requests.post(
                    f"{self.__nf_cloud_base_url}/api/projects/{project_id}/workflow-log",
                    auth=HTTPBasicAuth(
                        self.__nf_cloud_api_usr, self.__nf_cloud_api_pwd
                    ),
                    headers=headers,
                    timeout=self.__class__.TIMEOUT,
                    data=log,
                ) as response:
                    if not response.ok:
                        raise ValueError(f"Error posting weblog: {response.text}")
            except requests.exceptions.ConnectionError as e:
                if i < self.__class__.API_CALL_TRIES - 1:
                    logging.error(
                        "[WORKER / API CLIENT / ATTEMPT %i] Error while sending weblog to NFCloud API: %s",
                        i + 1,
                        e,
                    )
                    sleep(self.__class__.RETRY_TIMEOUT)
                    continue

                raise e
