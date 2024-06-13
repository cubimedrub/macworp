from httpx import Request

from ..base import Client, Test


class ListWorkflows(Test):
    name = "List workflows"

    description = "List workflows"

    @staticmethod
    def run(client: Client):
        client.as_user(
            Request("GET", "https://localhost:8000/workflow"),
            200,
            3
        )