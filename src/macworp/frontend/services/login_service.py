import os
import logging
from typing import Dict

import httpx

from macworp.configuration import Configuration


logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)


class LoginService:
    """Service class to handle authentication with the backend"""

    def __init__(self, config: Configuration):
        self.config = config
        self.backend_url = config.frontend.backend_url.rstrip("/")
        self.client = httpx.AsyncClient(timeout=30.0)

    async def get_login_providers(self) -> Dict:
        """Fetch available login providers from backend"""
        async with httpx.AsyncClient(
            verify=not self.config.frontend.skip_cert_verification
        ) as client:
            response = await client.get(f"{self.backend_url}/api/users/login-providers")
            print(f"Status: {response.status_code}")
            print(f"Response: {response.text}")
            if response.status_code == 200:
                return response.json()
            else:
                raise RuntimeError(
                    f"Fehler beim Abrufen der Projekte: {response.status_code} - {response.text}"
                )

    async def login_with_credentials(
        self, provider_type: str, provider: str, username: str, password: str
    ) -> Dict:
        """Login with username/password credentials"""
        try:
            login_data = {"login_id": username, "password": password}

            async with httpx.AsyncClient(
                verify=not self.config.frontend.skip_cert_verification
            ) as client:
                response = await client.post(
                    f"{self.backend_url}/api/users/login/{provider_type}/{provider}",
                    json=login_data,
                )
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
        # todo big uff much wow
        pass
