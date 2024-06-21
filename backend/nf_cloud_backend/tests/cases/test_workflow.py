from httpx import Request

from ..base import DONT_CARE, Test
from ..constants import *


NAME = "some name"
DESCRIPTION = "some description"
DEFINITION = {}
IS_PUBLISHED = True


class ListWorkflowsTest(Test):
    name = "List workflows"

    description = "List workflows"

    def test_this(self):
        self.as_default(
            Request("GET", self.endpoint("/workflow")),
            200,
            [
                WORKFLOW_1_OWNED,
                # WORKFLOW_2_PRIVATE is not visible to the default user
                WORKFLOW_3_READ_SHARED,
                WORKFLOW_4_WRITE_SHARED,
                WORKFLOW_5_PUBLIC
            ]
        )


class CreateWorkflowTest(Test):
    name = "Create workflow"

    description = "Creates a new workflow and then accesses it"

    def test_this(self):

        id = self.as_default(
            Request(
                "POST",
                self.endpoint("/workflow/new"),
                json={
                    "name": NAME,
                    "description": DESCRIPTION,
                    "definition": DEFINITION,
                    "is_published": IS_PUBLISHED,
                }
            ),
            200,
            DONT_CARE
        ).json()
        
        self.as_default(
            Request("GET", f"https://localhost:8000/workflow/{id}"),
            200,
            {
                "name": NAME,
                "owner_id": USER_2_DEFAULT,
                "description": DESCRIPTION,
                "definition": DEFINITION,
                "is_published": IS_PUBLISHED,
                "read_shared": [],
                "write_shared": [],
            }
        )


class ShowWorkflowTest(Test):
    name = "Show workflow"

    description = "Shows an existing workflow"

    def test_this(self):
        self.as_default(
            Request(
                "GET",
                self.endpoint(f"/workflow/{WORKFLOW_4_WRITE_SHARED}"),
            ),
            200,
            {
                "name": "write-shared",
                "owner_id": USER_1_ADMIN,
                "description": "",
                "definition": {},
                "is_published": False,
                "read_shared": [],
                "write_shared": [USER_2_DEFAULT],
            }
        )


class EditWorkflowTest(Test):
    name = "Edit workflow"

    description = "Edits an existing workflow and then shows it"

    def test_this(self):
        self.as_default(
            Request(
                "POST",
                self.endpoint(f"/workflow/{WORKFLOW_4_WRITE_SHARED}/edit"),
                json={
                    "name": NAME,
                    "description": DESCRIPTION,
                    "definition": DEFINITION,
                    "is_published": IS_PUBLISHED
                }
            ),
            200,
            None
        )

        self.as_default(
            Request(
                "GET",
                self.endpoint(f"/workflow/{WORKFLOW_4_WRITE_SHARED}"),
            ),
            200,
            {
                "name": NAME,
                "owner_id": USER_1_ADMIN,
                "description": DESCRIPTION,
                "definition": DEFINITION,
                "is_published": IS_PUBLISHED,
                "read_shared": [],
                "write_shared": [USER_2_DEFAULT],
            }
        )


class TransferWorkflowOwnershipTest(Test):
    name = "Transfer workflow ownership"

    description = "The default user transfers ownership of one of their workflows to the admin user. After the transfer, they will still have write access."

    def test_this(self):
        self.as_default(
            Request(
                "POST",
                self.endpoint(f"/workflow/{WORKFLOW_1_OWNED}/transfer_ownership?user_id={USER_1_ADMIN}"),
            ),
            200,
            None
        )

        self.as_default(
            Request(
                "GET",
                self.endpoint(f"/workflow/{WORKFLOW_1_OWNED}"),
            ),
            200,
            {
                "name": DONT_CARE,
                "owner_id": USER_1_ADMIN,
                "description": DONT_CARE,
                "definition": DONT_CARE,
                "is_published": DONT_CARE,
                "read_shared": [],
                "write_shared": [USER_2_DEFAULT],
            }
        )


# class DeleteWorkflowTest(Test):
#     name = "Delete workflow"

#     description = "Deletes a workflow and confirms that the workflow is in fact deleted"

#     def test_this(self):
#         self.as_default(
#             Request(
#                 "POST",
#                 self.endpoint(f"/workflow/{WORKFLOW_1_OWNED}/delete"),
#             ),
#             200,
#             None
#         )

#         self.as_default(
#             Request(
#                 "GET",
#                 self.endpoint(f"/workflow/{WORKFLOW_1_OWNED}"),
#             ),
#             422,
#             DONT_CARE
#         )