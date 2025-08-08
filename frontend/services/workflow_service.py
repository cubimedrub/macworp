import os

import httpx

BACKEND_URL = os.getenv("BACKEND_URL")
API_TOKEN = os.getenv("API_TOKEN")
AUTH_TYPE = os.getenv("AUTH_TYPE")

class WorkflowService:
    def __init__(self):
        self.workflows = []


    async def load_workflows(self):
        """loads the workflows"""
        headers = {}
        if API_TOKEN:
            headers[f"{AUTH_TYPE}"] = API_TOKEN
        async with httpx.AsyncClient() as client:
            response = await client.get(f"{BACKEND_URL}/workflow/",
                                        headers=headers)
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
            response = await client.get(f"{BACKEND_URL}/workflow/{workflow["id"]}",
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
            response = await client.delete(f"{BACKEND_URL}/workflow/{workflow["id"]}/delete",
                                        headers=headers
                                           )
            if response.status_code == 200:
                return True
            else:
                raise RuntimeError(f"Error while loading Projects: {response.status_code}")