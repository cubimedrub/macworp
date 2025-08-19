import httpx

from macworp.configuration import Configuration


class WorkflowService:
    def __init__(self, config: Configuration, auth_token: str):
        self.config = config
        self.auth_token = auth_token
        self.workflows = []

    async def load_workflows(self, project_id: int | None = None):
        """loads the workflows"""
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
                    f"Error while loading Projects: {response.status_code}"
                )

    async def get_workflow(self, workflow):
        """loads a single workflow from server"""
        headers = {"Authorization": f"Token {self.auth_token}"}
        async with httpx.AsyncClient() as client:
            response = await client.get(
                f"{self.config.frontend.backend_url}/workflow/{workflow["id"]}",
                headers=headers,
            )
            if response.status_code == 200:
                self.workflows = response.json()
                return self.workflows
            else:
                raise RuntimeError(
                    f"Error while loading Projects: {response.status_code}"
                )

    async def delete_workflow(self, workflow):
        headers = {"Authorization": f"Token {self.auth_token}"}
        async with httpx.AsyncClient() as client:
            response = await client.delete(
                f"{self.config.frontend.backend_url}/workflow/{workflow["id"]}/delete",
                headers=headers,
            )
            if response.status_code == 200:
                return True
            else:
                raise RuntimeError(
                    f"Error while loading Projects: {response.status_code}"
                )

    async def start_workflow(self, project_id, workflow, user_parameter_values):
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
