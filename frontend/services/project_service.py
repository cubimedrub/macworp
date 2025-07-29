import logging
import os
import httpx

BACKEND_URL = os.getenv("BACKEND_URL")
API_TOKEN = os.getenv("API_TOKEN")
AUTH_TYPE = os.getenv("AUTH_TYPE")

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)


class ProjectService:
    PROJECTS_PER_PAGE = 50

    def __init__(self):
        self.projects = []
        self.total_project_count = 0

    async def get_project_ids(self) -> list[int]:
        headers = {"Authorization": f"{AUTH_TYPE} {API_TOKEN}"} if API_TOKEN else {}
        async with httpx.AsyncClient() as client:
            response = await client.get(f"{BACKEND_URL}/project/", headers=headers)
            if response.status_code == 200:
                return response.json()
            else:
                raise RuntimeError(f"Error while loading any Projects: {response.status_code}")

    async def get_project_descriptions(self, project_id: int) -> list[dict[str, str]]:
        headers = {"Authorization": f"{AUTH_TYPE} {API_TOKEN}"} if API_TOKEN else {}
        async with httpx.AsyncClient() as client:
            response = await client.get(f"{BACKEND_URL}/project/{project_id}", headers=headers)
            if response.status_code == 200:
                return response.json()
            else:
                raise RuntimeError(f"Error loading Projects description: {response.status_code}")

    async def load_count(self):
        headers = {}
        if API_TOKEN:
            headers[f"{AUTH_TYPE}"] = API_TOKEN
        async with httpx.AsyncClient() as client:
            response = await client.get(f"{BACKEND_URL}/project/count", headers=headers)
            if response.status_code == 200:
                self.total_project_count = response.json()
                return self.total_project_count
            else:
                raise RuntimeError(f"Error loading Projects count: {response.status_code}{API_TOKEN}")

    async def load_project(self, project_id: int):
        """
        load a single Project from API
        """
        headers = {}
        if API_TOKEN:
            headers[f"{AUTH_TYPE}"] = API_TOKEN
        async with httpx.AsyncClient() as client:
            response = await client.get(f"{BACKEND_URL}/project/{project_id}",
                                        headers=headers)
            if response.status_code == 200:
                self.projects = response.json()
                return self.projects
            else:
                raise RuntimeError(f"Error while loading Project: {response.status_code}")

    async def load_projects(self, page: int) -> dict:
        """
        load a set of Projects from API
        """
        headers = {}
        if API_TOKEN:
            headers[f"{AUTH_TYPE}"] = API_TOKEN
        offset = (page - 1) * self.PROJECTS_PER_PAGE
        async with httpx.AsyncClient() as client:
            response = await client.get(f"{BACKEND_URL}/project/?offset={offset}&limit={self.PROJECTS_PER_PAGE}",
                                        headers=headers)
            if response.status_code == 200:
                self.projects = response.json()
                return self.projects
            else:
                raise RuntimeError(f"Error while loading Projects: {response.status_code}")
