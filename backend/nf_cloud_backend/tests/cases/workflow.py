from httpx import Request

from ..base import DONT_CARE, Client, Test
from ..constants import *

class ListWorkflows(Test):
    name = "List workflows"

    description = "List workflows"

    @staticmethod
    def run(client: Client):
        client.as_user(
            Request("GET", "https://localhost:8000/workflow"),
            200,
            [
                WORKFLOW_1_OWNED,
                WORKFLOW_3_READ_SHARED,
                WORKFLOW_4_WRITE_SHARED,
                WORKFLOW_5_PUBLIC
            ]
        )

class CreateWorkflow(Test):
    name = "Create workflow"

    description = "Create workflow"

    @staticmethod
    def run(client: Client):
        NAME = "some name"
        DESCRIPTION = "some description"
        DEFINITION = {}
        IS_PUBLISHED = False

        id = client.as_user(
            Request("POST", "https://localhost:8000/workflow/new", json={
                "name": NAME,
                "description": DESCRIPTION,
                "definition": DEFINITION,
                "is_published": IS_PUBLISHED,
            }),
            200,
            DONT_CARE
        ).json()

        client.as_user(
            Request("GET", f"https://localhost:8000/workflow/{id}"),
            200,
            {
                "name": NAME,
                "owner_id": USER_1_DEFAULT,
                "description": DESCRIPTION,
                "is_published": IS_PUBLISHED,
                "read_shared": [],
                "write_shared": [],
            }
        )