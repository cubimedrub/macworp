from httpx import Request

from ..base import DONT_CARE, Test, endpoint
from ..constants import *
from ..matrix import *


NAME = "some name"
DESCRIPTION = "some description"
DEFINITION = {}
IS_PUBLISHED = True


class ListWorkflowsTest(Test):
    def request(self) -> Request:
        return Request("GET", endpoint("/workflow"))

    def test_result(self):
        self.as_default(
            self.request(),
            200,
            [
                WORKFLOW_1_OWNED,
                # WORKFLOW_2_PRIVATE is not visible to the default user
                WORKFLOW_3_READ_SHARED,
                WORKFLOW_4_WRITE_SHARED,
                WORKFLOW_5_PUBLIC
            ]
        )

    @parameterized.expand([
        [UNAUTHENTICATED, 401],
        [DEFAULT, 200],
        [ADMIN, 200]
    ], name_func)
    def test_permissions(self, role, status):
        self.as_user(
            role.login,
            self.request(),
            status,
            DONT_CARE
        )


class CreateWorkflowTest(Test):
    def request(self) -> Request:
        return Request(
            "POST",
            endpoint("/workflow/new"),
            json={
                "name": NAME,
                "description": DESCRIPTION,
                "definition": DEFINITION,
                "is_published": IS_PUBLISHED,
            }
        )

    def test_result(self):
        id = self.as_default(
            self.request(),
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
    
    @parameterized.expand([
        [UNAUTHENTICATED, 401],
        [DEFAULT, 200],
        [ADMIN, 200]
    ], name_func)
    def test_permissions(self, role, status):
        self.as_user(
            role.login,
            self.request(),
            status,
            DONT_CARE
        )


class ShowWorkflowTest(Test):
    def request(self, workflow_id: int) -> Request:
        return Request(
            "GET",
            endpoint(f"/workflow/{workflow_id}"),
        )

    def test_result(self):
        self.as_default(
            self.request(WORKFLOW_4_WRITE_SHARED),
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

    @parameterized.expand([
        [UNAUTHENTICATED, 401],
        [PRIVATE, 403],
        [READ_ACCESS, 200],
        [WRITE_ACCESS, 200],
        [OWNED, 200],
        [ADMIN, 200]
    ], name_func)
    def test_permissions(self, role, status):
        self.as_user(
            role.login,
            self.request(role.workflow),
            status,
            DONT_CARE
        )


class EditWorkflowTest(Test):
    def request(self, workflow_id: int) -> Request:
        return Request(
            "POST",
            endpoint(f"/workflow/{workflow_id}/edit"),
            json={
                "name": NAME,
                "description": DESCRIPTION,
                "definition": DEFINITION,
                "is_published": IS_PUBLISHED
            }
        )

    def test_result(self):
        self.as_default(
            self.request(WORKFLOW_4_WRITE_SHARED),
            200,
            None
        )

        self.as_default(
            Request(
                "GET",
                endpoint(f"/workflow/{WORKFLOW_4_WRITE_SHARED}"),
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
    
    @parameterized.expand([
        [UNAUTHENTICATED, 401],
        [PRIVATE, 403],
        [READ_ACCESS, 403],
        [WRITE_ACCESS, 200],
        [OWNED, 200],
        [ADMIN, 200]
    ], name_func)
    def test_permissions(self, role, status):
        self.as_user(
            role.login,
            self.request(role.workflow),
            status,
            DONT_CARE
        )


class TransferWorkflowOwnershipTest(Test):
    def request(self, workflow_id: int) -> Request:
        return Request(
            "POST",
            endpoint(f"/workflow/{workflow_id}/transfer_ownership?user_id={USER_1_ADMIN}"),
        )

    def test_result(self):
        self.as_default(
            self.request(WORKFLOW_1_OWNED),
            200,
            None
        )

        self.as_default(
            Request(
                "GET",
                endpoint(f"/workflow/{WORKFLOW_1_OWNED}"),
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
    
    @parameterized.expand([
        [UNAUTHENTICATED, 401],
        [PRIVATE, 403],
        [READ_ACCESS, 403],
        [WRITE_ACCESS, 403],
        [OWNED, 200],
        [ADMIN, 200]
    ], name_func)
    def test_permissions(self, role, status):
        self.as_user(
            role.login,
            self.request(role.workflow),
            status,
            DONT_CARE
        )


class DeleteWorkflowTest(Test):
    def request(self, workflow_id: int) -> Request:
        return Request(
            "POST",
            endpoint(f"/workflow/{workflow_id}/delete"),
        )

    def test_result(self):
        self.as_default(
            self.request(WORKFLOW_1_OWNED),
            200,
            None
        )

        self.as_default(
            Request(
                "GET",
                endpoint(f"/workflow/{WORKFLOW_1_OWNED}"),
            ),
            422,
            DONT_CARE
        )
    
    @parameterized.expand([
        [UNAUTHENTICATED, 401],
        [PRIVATE, 403],
        [READ_ACCESS, 403],
        [WRITE_ACCESS, 403],
        [OWNED, 200],
        [ADMIN, 200]
    ], name_func)
    def test_permissions(self, role, status):
        self.as_user(
            role.login,
            self.request(role.workflow),
            status,
            DONT_CARE
        )


class AddWorkflowShareTest(Test):
    def request(self, workflow_id: int) -> Request:
        return Request(
            "POST",
            endpoint(f"/workflow/{workflow_id}/share/add?write=true"),
            json=[USER_2_DEFAULT]
        )

    def test_result(self):
        self.as_admin(
            self.request(WORKFLOW_3_READ_SHARED),
            200,
            None
        )

        self.as_admin(
            Request(
                "GET",
                endpoint(f"/workflow/{WORKFLOW_3_READ_SHARED}"),
            ),
            200,
            {
                "name": DONT_CARE,
                "owner_id": DONT_CARE,
                "description": DONT_CARE,
                "definition": DONT_CARE,
                "is_published": DONT_CARE,
                "read_shared": [],
                "write_shared": [USER_2_DEFAULT],
            }
        )
    
    @parameterized.expand([
        [UNAUTHENTICATED, 401],
        [PRIVATE, 403],
        [READ_ACCESS, 403],
        [WRITE_ACCESS, 200],
        [OWNED, 200],
        [ADMIN, 200]
    ], name_func)
    def test_permissions(self, role, status):
        self.as_user(
            role.login,
            self.request(role.workflow),
            status,
            DONT_CARE
        )


class RemoveWorkflowShareTest(Test):
    def request(self, workflow_id: int) -> Request:
        return Request(
            "POST",
            endpoint(f"/workflow/{workflow_id}/share/remove"),
            json=[USER_2_DEFAULT]
        )

    def test_result(self):
        self.as_admin(
            self.request(WORKFLOW_3_READ_SHARED),
            200,
            None
        )

        self.as_admin(
            Request(
                "GET",
                endpoint(f"/workflow/{WORKFLOW_3_READ_SHARED}"),
            ),
            200,
            {
                "name": DONT_CARE,
                "owner_id": DONT_CARE,
                "description": DONT_CARE,
                "definition": DONT_CARE,
                "is_published": DONT_CARE,
                "read_shared": [],
                "write_shared": [],
            }
        )
    
    @parameterized.expand([
        [UNAUTHENTICATED, 401],
        [PRIVATE, 403],
        [READ_ACCESS, 403],
        [WRITE_ACCESS, 200],
        [OWNED, 200],
        [ADMIN, 200]
    ], name_func)
    def test_permissions(self, role, status):
        self.as_user(
            role.login,
            self.request(role.workflow),
            status,
            DONT_CARE
        )