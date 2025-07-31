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

    async def delete_project(self, project_id):
        """
        delete project
        """
        headers = {}
        if API_TOKEN:
            headers[f"{AUTH_TYPE}"] = API_TOKEN
        async with httpx.AsyncClient() as client:
            response = await client.delete(f"{BACKEND_URL}/project/{project_id}/delete",
                                           headers=headers)
            if response.status_code == 200:
                self.projects = response.json()
                return self.projects
            else:
                raise RuntimeError(f"Error while loading Projects: {response.status_code}")

    def clean_project_data(self, project):
        """
        cleanse poroject data
        """
        cleaned = {}
        for key, value in project.items():
            if value == "":
                continue

            if key == "workflow_id":
                if value == "" or value is None:
                    continue
                elif value == "unset":
                    cleaned[key] = "unset"
                else:
                    try:
                        cleaned[key] = int(value)
                    except (ValueError, TypeError):
                        continue

            elif key == "is_published":
                if isinstance(value, bool):
                    cleaned[key] = value
                elif isinstance(value, str):
                    cleaned[key] = value.lower() in ('true', '1', 'yes', 'on')


            elif key in ["name", "description"]:
                if value and value.strip():
                    cleaned[key] = value.strip()

            else:
                cleaned[key] = value

        return cleaned

    async def update_project(self, project, project_id):
        """
        edits project data
        """
        cleaned_data = self.clean_project_data(project)
        headers = {}
        if API_TOKEN:
            headers[f"{AUTH_TYPE}"] = API_TOKEN
        async with httpx.AsyncClient() as client:
            response = await client.post(f"{BACKEND_URL}/project/{project_id}/edit",
                                         headers=headers,
                                         json=cleaned_data)
            if response.status_code == 200:
                return True
            else:
                # Detaillierte Fehlerinformationen ausgeben
                error_detail = response.text
                try:
                    error_json = response.json()
                    print(f"Error details: {error_json}")
                except:
                    print(f"Error text: {error_detail}")

                raise RuntimeError(f"Error while editing project: {response.status_code} - {error_detail}")

    async def get_file_path(self, project_id):
        """returns files and Path"""
        headers = {}
        if API_TOKEN:
            headers[f"{AUTH_TYPE}"] = API_TOKEN
        async with httpx.AsyncClient() as client:
            response = await client.get(f"{BACKEND_URL}/project/{project_id}/files",
                                        headers=headers)

            if response.status_code == 200:
                data = response.json()
                files = data.get("files", [])
                file_paths = []
                for filename in files:
                    file_paths.append(filename)  # Für die API reicht der Dateiname
                return file_paths
            else:
                error_detail = response.text
            try:
                error_json = response.json()
                print(f"Error details: {error_json}")
            except:
                print(f"Error text: {error_detail}")

            raise RuntimeError(f"Error while editing project: {response.status_code} - {error_detail}")


