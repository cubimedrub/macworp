"""Module to communicate with the MAcWorP API."""

# std imports
import logging
from time import sleep
from typing import ClassVar, Dict

# 3rd party imports
import requests
from requests.auth import HTTPBasicAuth

from macworp_utils.constants import (
    WEBLOG_WORKFLOW_ENGINE_HEADER,
    SupportedWorkflowEngine,
)


class BackendWebApiClient:
    """
    Client for communicating with the MAcWorP API.
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
        self,
        macworp_base_url: str,
        macworp_api_user: str,
        macworp_api_password: str,
        verify_cert: bool,
    ):
        """
        Creates a new BackendWebApiClient.

        Parameters
        ----------
        macworp_base_url : str
            Base URL of the MAcWorP API
        macworp_api_user : str
            Username for the MAcWorP API
        macworp_api_password : str
            Password for the MAcWorP API
        verify_cert : bool
            Whether to verify the certificate
        """
        self.__macworp_base_url = macworp_base_url
        self.__macworp_api_usr = macworp_api_user
        self.__macworp_api_pwd = macworp_api_password
        self.__verify_cert = verify_cert

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
        url = f"{self.__macworp_base_url}/api/workflows/{workflow_id}"
        for i in range(self.__class__.API_CALL_TRIES):
            try:
                with requests.get(
                    url,
                    headers=self.__class__.HEADERS,
                    timeout=self.__class__.TIMEOUT,
                    verify=self.__verify_cert,
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

    def is_project_ignored(self, project_id: int) -> bool:
        """
        Check if project is currently ignored

        Parameters
        ----------
        project_id : int
            Project ID

        Returns
        -------
        Bool
            Project not found also returns True

        Raises
        ------
        ValueError
            If the request was not successful.
        """
        url = f"{self.__macworp_base_url}/api/projects/{project_id}/is-ignored"
        for i in range(self.__class__.API_CALL_TRIES):
            try:
                with requests.get(
                    url,
                    auth=HTTPBasicAuth(self.__macworp_api_usr, self.__macworp_api_pwd),
                    headers=self.__class__.HEADERS,
                    timeout=self.__class__.TIMEOUT,
                    verify=self.__verify_cert,
                ) as response:
                    print(
                        f"Checking ignore status for project {project_id}: {response.text} - {response.status_code}"
                    )
                    logging.debug(
                        f"Checking ignore status for project {project_id}: {response.text} - {response.status_code}"
                    )
                    match response.status_code:
                        case 200:
                            return True
                        case 204:
                            return False
                        case 404:
                            return True
                        case _:
                            raise ValueError(
                                f"Error getting ignore status: {response.text}"
                            )
            except requests.exceptions.ConnectionError as e:
                if i < self.__class__.API_CALL_TRIES - 1:
                    logging.error(
                        (
                            "[WORKER / API CLIENT / ATTEMPT %i]"
                            "Error while getting workflow from API: %s"
                        ),
                        i + 1,
                        e,
                    )
                    sleep(self.__class__.RETRY_TIMEOUT)
                    continue

                raise e
        return False

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
        url = f"{self.__macworp_base_url}/api/projects/{project_id}/finished"
        for i in range(self.__class__.API_CALL_TRIES):
            try:
                with requests.post(
                    url,
                    auth=HTTPBasicAuth(self.__macworp_api_usr, self.__macworp_api_pwd),
                    headers=self.__class__.HEADERS,
                    timeout=self.__class__.TIMEOUT,
                    verify=self.__verify_cert,
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

    def post_weblog(
        self, project_id: int, workflow_engine: SupportedWorkflowEngine, log: bytes
    ):
        """
        Posts a web log entry.

        Parameters
        ----------
        project_id : int
            Project ID
        workflow_engine : SupportedWorkflowEngine
            Workflow engine type
        log : Dict[str, str]
            Log entry

        Raises
        ------
        ValueError
            If the request was not successful.
        """
        headers = self.__class__.HEADERS.copy()
        headers["Content-Type"] = "application/json"
        headers[WEBLOG_WORKFLOW_ENGINE_HEADER] = str(workflow_engine)

        for i in range(self.__class__.API_CALL_TRIES):
            try:
                with requests.post(
                    f"{self.__macworp_base_url}/api/projects/{project_id}/workflow-log",
                    auth=HTTPBasicAuth(self.__macworp_api_usr, self.__macworp_api_pwd),
                    headers=headers,
                    timeout=self.__class__.TIMEOUT,
                    verify=self.__verify_cert,
                    data=log,
                ) as response:
                    if not response.ok:
                        raise ValueError(f"Error posting weblog: {response.text}")
            except requests.exceptions.ConnectionError as e:
                if i < self.__class__.API_CALL_TRIES - 1:
                    logging.error(
                        "[WORKER / API CLIENT / ATTEMPT %i] Error while sending weblog to MAcWorP API: %s",
                        i + 1,
                        e,
                    )
                    sleep(self.__class__.RETRY_TIMEOUT)
                    continue

                raise e
