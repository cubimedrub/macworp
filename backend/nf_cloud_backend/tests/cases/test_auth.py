from httpx import Request

from ..base import DONT_CARE, Test, endpoint


class FileBasedAuthenticationTest(Test):
    def test_this(self):
        self.send(
            Request(
                "POST",
                endpoint("/users/login/file/dev"),
                json={
                    "login_id": "devadmin",
                    "password": "developer"
                }
            ),
            200,
            { "jwt": DONT_CARE }
        )


class DatabaseAuthenticationTest(Test):
    def test_this(self):
        self.send(
            Request(
                "POST",
                endpoint("/users/login/database/local"),
                json={
                    "login_id": "testperson",
                    "password": "password"
                }
            ),
            200,
            { "jwt": DONT_CARE }
        )