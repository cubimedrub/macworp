from typing import Dict, List, Optional, Any

import httpx

from macworp.configuration import Configuration


class WorkflowService:
    """
    Service class for managing workflow operations via API calls.

    This service handles all workflow-related operations including loading,
    creating, editing, deleting, and starting workflows. It communicates
    with the backend API using HTTP requests with token-based authentication.

    Attributes:
        config (Configuration): Application configuration containing backend URL
            and other settings.
        auth_token (str): Authentication token for API requests.
        workflows (List[Dict]): Cached list of workflows loaded from the server.
    """

    def __init__(self, config: Configuration, auth_token: str):
        """
        Initialize the WorkflowService with configuration and authentication.

        Args:
            config (Configuration): Application configuration object containing
                backend URL and other necessary settings.
            auth_token (str): Authentication token to be used for all API requests
                in the format expected by the backend (typically Bearer or Token).
        """
        self.config = config
        self.auth_token = auth_token
        self.workflows = []

    async def load_workflows(self, project_id: Optional[int] = None) -> List[Dict[str, Any]]:
        """
        Load workflows from the server, optionally filtered by project.

        Fetches workflow data from the backend API and caches it in the workflows
        attribute. Can optionally filter workflows by project ID.

        Args:
            project_id (Optional[int]): ID of the project to filter workflows by.
                If None, loads all workflows accessible to the authenticated user.

        Returns:
            List[Dict[str, Any]]: List of workflow dictionaries containing workflow
                data such as ID, name, description, and configuration.

        Raises:
            RuntimeError: If the API request fails with a non-200 status code.
                The error message includes the HTTP status code for debugging.
        """
        headers = {"Authorization": f"Token {self.auth_token}"}

        params = {}
        if project_id:
            params["project_id"] = project_id

        async with httpx.AsyncClient() as client:
            response = await client.get(
                f"{self.config.frontend.backend_url}/workflow/",
                headers=headers,
                params=params,
            )
            if response.status_code == 200:
                self.workflows = response.json()
                return self.workflows
            else:
                raise RuntimeError(
                    f"Error while loading Workflows: {response.status_code}"
                )

    async def get_workflow(self, workflow: Dict[str, Any]) -> Dict[str, Any]:
        """
        Load a single workflow's detailed information from the server.

        Fetches detailed information for a specific workflow by its ID.

        Args:
            workflow (Dict[str, Any]): Workflow dictionary that must contain
                an 'id' key with the workflow's unique identifier.

        Returns:
            Dict[str, Any]: Detailed workflow information including configuration,
                parameters, and metadata.

        Raises:
            RuntimeError: If the API request fails with a non-200 status code.
                The error message includes the HTTP status code for debugging.
            KeyError: If the workflow dictionary doesn't contain an 'id' key.
        """
        headers = {"Authorization": f"Token {self.auth_token}"}
        async with httpx.AsyncClient() as client:
            response = await client.get(
                f"{self.config.frontend.backend_url}/workflow/{workflow['id']}",
                headers=headers,
            )
            if response.status_code == 200:
                workflow_data = response.json()
                return workflow_data
            else:
                raise RuntimeError(
                    f"Error while loading Workflow: {response.status_code}"
                )

    async def delete_workflow(self, workflow: Dict[str, Any]) -> bool:
        """
        Delete a workflow from the server.

        Permanently removes a workflow from the system. This operation
        cannot be undone.

        Args:
            workflow (Dict[str, Any]): Workflow dictionary that must contain
                an 'id' key with the workflow's unique identifier.

        Returns:
            bool: True if the workflow was successfully deleted.

        Raises:
            RuntimeError: If the API request fails with a non-200 status code.
                The error message includes the HTTP status code for debugging.
            KeyError: If the workflow dictionary doesn't contain an 'id' key.
        """
        headers = {"Authorization": f"Token {self.auth_token}"}
        async with httpx.AsyncClient() as client:
            response = await client.delete(
                f"{self.config.frontend.backend_url}/workflow/{workflow['id']}/delete",
                headers=headers,
            )
            if response.status_code == 200:
                return True
            else:
                raise RuntimeError(
                    f"Error while deleting Workflow: {response.status_code}"
                )

    async def start_workflow(self, project_id: int, workflow: Dict[str, Any],
                             user_parameter_values: Dict[str, Any]) -> bool:
        """
        Schedule and start a workflow execution within a project.

        Initiates workflow execution with the provided parameter values.
        The workflow will be scheduled to run in the context of the specified project.

        Args:
            project_id (int): ID of the project where the workflow should be executed.
            workflow (Dict[str, Any]): Workflow dictionary that must contain
                an 'id' key with the workflow's unique identifier.
            user_parameter_values (Dict[str, Any]): Dictionary containing parameter
                names as keys and their corresponding values. Must match the
                workflow's expected parameter schema.

        Returns:
            bool: True if the workflow was successfully scheduled for execution.

        Raises:
            RuntimeError: If the API request fails with a non-200 status code.
                The error message includes both the HTTP status code and response
                text for detailed debugging information.
            KeyError: If the workflow dictionary doesn't contain an 'id' key.
        """
        headers = {"Authorization": f"Token {self.auth_token}"}

        async with httpx.AsyncClient() as client:
            response = await client.post(
                f"{self.config.frontend.backend_url}/project/{project_id}/schedule/{workflow['id']}",
                headers=headers,
                json=user_parameter_values,
            )
            if response.status_code == 200:
                return True
            else:
                raise RuntimeError(
                    f"Error while Scheduling Workflow: {response.status_code} - {response.text}"
                )

    async def edit_workflow(self, workflow: Dict[str, Any],
                            user_parameter_values: Dict[str, Any]) -> bool:
        """
        Update an existing workflow's configuration and parameters.

        Modifies an existing workflow with new parameter values or configuration
        changes. The workflow must already exist in the system.

        Args:
            workflow (Dict[str, Any]): Workflow dictionary that must contain
                an 'id' key with the workflow's unique identifier.
            user_parameter_values (Dict[str, Any]): Dictionary containing the
                updated workflow configuration, parameters, and metadata.

        Returns:
            bool: True if the workflow was successfully updated.

        Raises:
            RuntimeError: If the API request fails with a non-200 status code.
                The error message includes both the HTTP status code and response
                text for detailed debugging information.
            KeyError: If the workflow dictionary doesn't contain an 'id' key.
        """
        headers = {"Authorization": f"Token {self.auth_token}"}

        async with httpx.AsyncClient() as client:
            response = await client.post(
                f"{self.config.frontend.backend_url}/workflow/{workflow['id']}/edit",
                headers=headers,
                json=user_parameter_values
            )
            if response.status_code == 200:
                return True
            else:
                raise RuntimeError(
                    f"Error while Editing Workflow: {response.status_code} - {response.text}"
                )

    async def save_workflow(self, workflow_data: Dict[str, Any]) -> bool:
        headers = {"Authorization": f"Token {self.auth_token}"}

        # Parse the definition if it's a JSON string
        definition = workflow_data.get("definition")
        if isinstance(definition, str) and definition.strip():
            try:
                import json
                definition = json.loads(definition)
            except json.JSONDecodeError as e:
                raise ValueError(f"Invalid JSON in definition field: {str(e)}")
        elif not definition:
            definition = {}  # Default to empty dict if no definition

        payload = {
            "name": workflow_data.get("name"),
            "description": workflow_data.get("description", ""),
            "definition": definition,
            "is_published": workflow_data.get("is_published", False)
        }

        async with httpx.AsyncClient() as client:
            response = await client.post(
                f"{self.config.frontend.backend_url}/workflow/new",
                headers=headers,
                json=payload
            )

            if response.status_code == 201:
                return True
            else:
                raise RuntimeError(
                    f"Error while Saving Workflow: {response.status_code} - {response.text}"
                )

    async def publish_workflow(self, workflow):
        headers = {"Authorization": f"Token {self.auth_token}"}

        async with httpx.AsyncClient() as client:
            response = await client.post(
                f"{self.config.frontend.backend_url}/workflow/{workflow['id']}/publish",
                headers=headers,
                json=workflow
            )
        if response.status_code == 201:
            return True
        else:
            raise RuntimeError(
                f"Error while publishing Workflow: {response.status_code} - {response.text}"
            )
