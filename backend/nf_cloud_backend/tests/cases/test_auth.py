from httpx import Request

from ..base import DONT_CARE, Test


class FileBasedAuthenticationTest(Test):
    name = "File-based authentication"

    description = "Tries logging in using file-based authentication."

    def test_this(self):
        self.send(
            Request(
                "POST",
                self.endpoint("/users/login/file/dev"),
                json={
                    "login_id": "devadmin",
                    "password": "developer"
                }
            ),
            200,
            { "jwt": DONT_CARE }
        )


class DatabaseAuthenticationTest(Test):
    name = "Database authentication"

    description = "Tries logging in using database authentication."

    def test_this(self):
        self.send(
            Request(
                "POST",
                self.endpoint("/users/login/database/local"),
                json={
                    "login_id": "testperson",
                    "password": "password"
                }
            ),
            200,
            { "jwt": DONT_CARE }
        )