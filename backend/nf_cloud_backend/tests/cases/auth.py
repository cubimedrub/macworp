from httpx import Request

from ..base import DONT_CARE, Client, Test


class FileBasedAuthentication(Test):
    name = "File-based authentication"

    description = "Tries logging in using file-based authentication."

    @staticmethod
    def run(client: Client):
        token: str = client.send(
            Request("POST", "https://localhost:8000/users/login/file/dev", json={ "login_id": "devadmin", "password": "developer" }),
            200,
            { "jwt": DONT_CARE }
        ).json()["jwt"]


class DatabaseAuthentication(Test):
    name = "Database authentication"

    description = "Tries logging in using database authentication."

    @staticmethod
    def run(client: Client):
        token: str = client.send(
            Request("POST", "https://localhost:8000/users/login/database/local", json={ "login_id": "devadmin", "password": "developer" }),
            200,
            { "jwt": DONT_CARE }
        ).json()["jwt"]
