from unittest import TestCase
from httpx import Request, Response
from typing import Any

from fastapi.testclient import TestClient


class DontCare:
    def __init__(self):
        pass

    def __str__(self):
        return "..."

DONT_CARE = DontCare()

PartialJson = None | bool | int | float | str | list["PartialJson"] | dict[str, "PartialJson"] | DontCare
"""
Can be a JSON value or the special type DontCare, which signifies that any value present is fine.
"""

def match_partial_json(actual: Any, expected: PartialJson) -> bool:
    if isinstance(expected, DontCare):
        return True    
    if isinstance(actual, list) and isinstance(expected, list):
        if len(actual) != len(expected):
            return False
        for i in range(len(actual)):
            if not match_partial_json(actual[i], expected[i]):
                return False
        return True
    if isinstance(actual, dict) and isinstance(expected, dict):
        if actual.keys() != expected.keys():
            return False
        for key in actual.keys():
            if not match_partial_json(actual[key], expected[key]):
                return False
        return True
    return actual == expected


class LogEntry:
    request: Request
    status: int
    json: Any

    def __init__(self, request: Request, status: int, json: Any):
        self.request = request
        self.status = status
        self.json = json


class Client:
    client: TestClient
    test_case: TestCase
    logs: list[LogEntry]

    def __init__(self, client: TestClient, test_case: TestCase):
        self.client = client
        self.test_case = test_case
        self.logs = []
        self._default_token = None
        self._admin_token = None

    def send(self, request: Request, status: int, json: PartialJson) -> Response:
        print("Sending request:", request)
        response = self.client.send(request)
        actual_status = response.status_code
        actual_json = response.json()
        print("Got response:", actual_status, actual_json)
        
        self.test_case.assertEqual(actual_status, status)
        self.test_case.assert_(match_partial_json(actual_json, json)) 
        self.logs.append(LogEntry(request, actual_status, actual_json))
        return response
    
    _default_token: str | None
    @property
    def default_token(self) -> str:
        """
        Returns the token of the seeded default user. The token is cached, so only one authentication request will be performed.
        """
        
        if self._default_token is not None:
            return self._default_token
        return self.send(
            Request("POST", "https://localhost:8000/users/login/file/dev", json={ "login_id": "devuser", "password": "developer" }),
            200,
            { "jwt": DONT_CARE }
        ).json()["jwt"]
    
    
    def as_user(self, request: Request, status: int, json: PartialJson) -> Response:
        request.headers["X-Token"] = self.default_token
        return self.send(request, status, json)


    _admin_token: str | None
    @property
    def admin_token(self) -> str:
        """
        Returns the token of the seeded admin user. The token is cached, so only one authentication request will be performed.
        """
        
        if self._admin_token is not None:
            return self._admin_token
        return self.send(
            Request("POST", "https://localhost:8000/users/login/file/dev", json={ "login_id": "devadmin", "password": "developer" }),
            200,
            { "jwt": DONT_CARE }
        ).json()["jwt"]
    

    def as_admin(self, request: Request, status: int, json: PartialJson) -> Response:
        request.headers["X-Token"] = self.default_token
        return self.send(request, status, json)


class Test:
    name: str | None = None
    """
    The name of the test.
    """

    description: str | None = None
    """
    A description of what this test does.
    """

    @staticmethod
    def run(client: Client):
        raise NotImplementedError()