import os

import httpx


BACKEND_URL = os.getenv("BACKEND_URL", "GET http://localhost:8000/project/")
API_TOKEN = os.getenv("API_TOKEN")

async def get_project_ids() -> list[int]:
    headers = {"Authorization": f"Bearer {API_TOKEN}"} if API_TOKEN else {}

    async with httpx.AsyncClient() as client:

        response = await client.get(BACKEND_URL, headers=headers)

        if response.status_code == 200:
            return response.json()
        else:
            raise RuntimeError(f"Fehler beim Abrufen der Projekte: {response.status_code}")