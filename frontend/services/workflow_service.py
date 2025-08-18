import os

import httpx

BACKEND_URL = os.getenv("BACKEND_URL")
API_TOKEN = os.getenv("API_TOKEN")
AUTH_TYPE = os.getenv("AUTH_TYPE")


class WorkflowService:
    def __init__(self):
        self.workflows = []

    async def load_workflows(self, project_id: int | None = None):
        """loads the workflows"""
        headers = {}
        if API_TOKEN:
            headers[f"{AUTH_TYPE}"] = API_TOKEN

        params = {}
        if project_id:
            params['project_id'] = project_id

        async with httpx.AsyncClient() as client:
            response = await client.get(f"{BACKEND_URL}/workflow/",
                                        headers=headers,
                                        params=params)
            if response.status_code == 200:
                self.workflows = response.json()
                return self.workflows
            else:
                raise RuntimeError(f"Error while loading Projects: {response.status_code}")

    async def get_workflow(self, workflow):
        """loads a single workflow from server"""
        headers = {}
        if API_TOKEN:
            headers[f"{AUTH_TYPE}"] = API_TOKEN
        async with httpx.AsyncClient() as client:
            response = await client.get(f"{BACKEND_URL}/workflow/{workflow['id']}",
                                        headers=headers)
            if response.status_code == 200:
                self.workflows = response.json()
                return self.workflows
            else:
                raise RuntimeError(f"Error while loading Projects: {response.status_code}")

    async def delete_workflow(self, workflow):
        headers = {}
        if API_TOKEN:
            headers[f"{AUTH_TYPE}"] = API_TOKEN
        async with httpx.AsyncClient() as client:
            response = await client.delete(f"{BACKEND_URL}/workflow/{workflow['id']}/delete",
                                           headers=headers)
            if response.status_code == 200:
                return True
            else:
                raise RuntimeError(f"Error while loading Projects: {response.status_code}")

    async def start_workflow(self, project_id, workflow, user_parameter_values):
        headers = {}

        if API_TOKEN:
            headers[f"{AUTH_TYPE}"] = API_TOKEN

        async with httpx.AsyncClient() as client:
            response = await client.post(
                f"{BACKEND_URL}/project/{project_id}/schedule/{workflow['id']}",
                headers=headers,
                json=user_parameter_values
            )
            if response.status_code == 200:
                return True
            else:
                raise RuntimeError(f"Error while Scheduling Workflow: {response.status_code} - {response.text}")
