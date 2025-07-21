import asyncio
import json
import os
from typing import Dict, List, Optional
import httpx
from nicegui import ui, app
from nicegui.events import ValueChangeEventArguments
import logging

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

BACKEND_URL = os.getenv("BACKEND_URL")
AUTH_TYPE = os.getenv("AUTH_TYPE")
API_TOKEN = os.getenv("API_TOKEN")


class LoginService:
    """Service class to handle authentication with the backend"""

    def __init__(self):
        self.backend_url = BACKEND_URL.rstrip('/')
        self.client = httpx.AsyncClient(timeout=30.0)

    async def get_login_providers(self) -> Dict:
        """Fetch available login providers from backend"""
        headers = {"Authorization": f"{AUTH_TYPE} {API_TOKEN}"} if API_TOKEN else {}
        async with httpx.AsyncClient() as client:
            response = await client.get(f"{BACKEND_URL}/users/login-providers", headers=headers)
            print(f"Status: {response.status_code}")
            print(f"Response: {response.text}")
            if response.status_code == 200:
                return response.json()
            else:
                raise RuntimeError(f"Fehler beim Abrufen der Projekte: {response.status_code} - {response.text}")

    async def login_with_credentials(self, provider_type: str, provider: str,
                                     username: str, password: str) -> Dict:
        headers = {"Authorization": f"{AUTH_TYPE} {API_TOKEN}"} if API_TOKEN else {}
        """Login with username/password credentials"""
        try:
            login_data = {
                "login_id": username,
                "password": password
            }

            async with httpx.AsyncClient() as client:
                response = await client.post(f"{BACKEND_URL}/users/login/{provider_type}/{provider}",
                                             headers=headers,
                                             json=login_data)
            if response.status_code == 200:
                return response.json()
            else:
                error_detail = response.json().get("detail", "Login failed")
                return {"error": error_detail}

        except httpx.RequestError as e:
            logger.error(f"Login request failed: {e}")
            return {"error": "Network error occurred"}
        except Exception as e:
            logger.error(f"Unexpected login error: {e}")
            return {"error": "An unexpected error occurred"}

    async def initiate_oauth_login(self, selected_provider_type, selected_provider):
        #todo big uff much wow
        pass


