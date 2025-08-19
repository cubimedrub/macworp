import json
from typing import Optional, Dict, Any

import httpx
from nicegui import ui

from macworp.configuration import Configuration

# TODO: This is not used at all?


class AuthService:
    def __init__(self, config: Configuration):
        self.config = config
        self.base_url = (
            f"{self.config.frontend.backend_url}/api/{self.config.frontend.api_version}"
        )
        self.token_key = "auth_token"
        self.user_key = "user_data"

    def _get_session_storage(self, key: str) -> Optional[str]:
        """Hole Daten aus dem Browser Session Storage"""
        try:
            if not hasattr(self, "_session_data"):
                self._session_data = {}
            return self._session_data.get(key)
        except:
            return None

    def _set_session_storage(self, key: str, value: str):
        """Speichere Daten im Browser Session Storage"""
        try:
            if not hasattr(self, "_session_data"):
                self._session_data = {}
            self._session_data[key] = value
        except:
            pass

    def _remove_session_storage(self, key: str):
        """Entferne Daten aus dem Browser Session Storage"""
        try:
            if hasattr(self, "_session_data") and key in self._session_data:
                del self._session_data[key]
        except:
            pass

    async def get_login_providers(self) -> Dict[str, Any]:
        """Hole verfügbare Login-Provider"""
        try:
            async with httpx.AsyncClient() as client:
                response = await client.get(f"{self.base_url}/users/login-providers")

                if response.status_code == 200:
                    return {"success": True, "data": response.json()}
                else:
                    return {"success": False, "error": "Konnte Provider nicht laden"}

        except Exception as e:
            return {"success": False, "error": f"Verbindungsfehler: {str(e)}"}

    async def login(
        self,
        username: str,
        password: str,
        provider_type: str = "database",
        provider: str = "default",
    ) -> Dict[str, Any]:
        """Login beim Backend mit Provider-System"""
        try:
            async with httpx.AsyncClient() as client:
                # Login-Request nach deinem Backend-Format
                login_data = {"username": username, "password": password}

                response = await client.post(
                    f"{self.base_url}/users/login/{provider_type}/{provider}",
                    json=login_data,
                    headers={"Content-Type": "application/json"},
                )

                if response.status_code == 200:
                    data = response.json()

                    if "jwt" in data:
                        self._set_session_storage(self.token_key, data["jwt"])

                        try:
                            user_data = self._decode_jwt_payload(data["jwt"])
                            if user_data:
                                self._set_session_storage(
                                    self.user_key, json.dumps(user_data)
                                )
                        except:
                            pass

                    return {"success": True, "data": data}
                else:
                    error_detail = "Login fehlgeschlagen"
                    try:
                        error_data = response.json()
                        error_detail = error_data.get("detail", error_detail)
                    except:
                        pass

                    return {"success": False, "error": error_detail}

        except Exception as e:
            return {"success": False, "error": f"Verbindungsfehler: {str(e)}"}

    def _decode_jwt_payload(self, token: str) -> Optional[Dict[str, Any]]:
        """Decode JWT Payload (ohne Verifikation, nur für User-Daten)"""
        try:
            import base64

            parts = token.split(".")
            if len(parts) != 3:
                return None

            payload = parts[1]
            padding = len(payload) % 4
            if padding:
                payload += "=" * (4 - padding)

            decoded_bytes = base64.urlsafe_b64decode(payload)
            payload_data = json.loads(decoded_bytes.decode("utf-8"))

            return payload_data
        except Exception:
            return None

    async def logout(self) -> Dict[str, Any]:
        """Logout - nur lokale Session löschen"""
        try:
            self._remove_session_storage(self.token_key)
            self._remove_session_storage(self.user_key)

            return {"success": True}

        except Exception as e:
            self._remove_session_storage(self.token_key)
            self._remove_session_storage(self.user_key)
            return {"success": False, "error": str(e)}

    def get_token(self) -> Optional[str]:
        """Hole aktuellen Auth-Token"""
        return self._get_session_storage(self.token_key)

    def get_user(self) -> Optional[Dict[str, Any]]:
        """Hole aktuelle User-Daten"""
        user_data = self._get_session_storage(self.user_key)
        if user_data:
            try:
                return json.loads(user_data)
            except:
                return None
        return None

    def is_authenticated(self) -> bool:
        """Prüfe ob User eingeloggt ist"""
        token = self.get_token()
        return token is not None

    async def verify_token(self) -> bool:
        """Verifiziere Token - JWT ist self-contained"""
        token = self.get_token()
        if not token:
            return False

        try:
            payload = self._decode_jwt_payload(token)
            if not payload:
                return False

            import time

            current_time = time.time()
            exp = payload.get("exp")

            if exp and current_time > exp:
                await self.logout()
                return False

            return True

        except Exception:
            return False

    async def get_headers(self) -> Dict[str, str]:
        """Hole Authorization Headers für API-Calls"""
        token = self.get_token()
        if token:
            return {"Authorization": f"Bearer {token}"}
        return {}

    async def api_request(self, method: str, endpoint: str, **kwargs) -> Dict[str, Any]:
        """Authenticated API Request"""
        try:
            headers = await self.get_headers()

            if "headers" in kwargs:
                headers.update(kwargs["headers"])
            kwargs["headers"] = headers

            async with httpx.AsyncClient() as client:
                response = await client.request(
                    method, f"{self.base_url}{endpoint}", **kwargs
                )

                if response.status_code == 401:
                    await self.logout()
                    return {"success": False, "error": "Session abgelaufen"}

                if response.status_code >= 400:
                    try:
                        error_data = response.json()
                        error_msg = error_data.get(
                            "detail", f"HTTP {response.status_code}"
                        )
                    except:
                        error_msg = f"HTTP {response.status_code}"
                    return {"success": False, "error": error_msg}

                return {"success": True, "data": response.json()}

        except Exception as e:
            return {"success": False, "error": str(e)}

    def require_auth(self, redirect_to: str = "/login"):
        """Decorator für geschützte Routen"""

        def decorator(func):
            async def wrapper(*args, **kwargs):
                if not self.is_authenticated():
                    ui.navigate.to(redirect_to)
                    return None

                if not await self.verify_token():
                    ui.navigate.to(redirect_to)
                    return None

                return await func(*args, **kwargs)

            return wrapper

        return decorator
