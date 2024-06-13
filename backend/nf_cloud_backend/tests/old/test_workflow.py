from . import DbTestCase
from .constants import *


class WorkflowTest(DbTestCase):
    def test_list(self):
        response = self.client.get("/workflow", headers=self.headers_default())

        self.assertEqual(response.status_code, 200)
        self.assertEqual(response.json(), [
            WORKFLOW_1_OWNED,
            WORKFLOW_3_READ_SHARED,
            WORKFLOW_4_WRITE_SHARED,
            WORKFLOW_5_PUBLIC
        ])

    
    def test_new(self):
        response = self.client.post("/workflow/new", headers=self.headers_default(), json={
            "name": "newname",
        })

        self.assertEqual(response.status_code, 200)
        id = response.json()
        self.assertIsInstance(id, int)
        
        get_response = self.client.get(f"/workflow/{id}", headers=self.headers_default())
        self.assertEqual(get_response.status_code, 200)
        self.assertEqual(get_response.json(), {
            "name": "newname",
            "owner_id": USER_1_DEFAULT,
            "description": "",
            "definition": {},
            "is_published": False
        })

    
    def test_get(self):
        response_403 = self.client.get(f"/workflow/{WORKFLOW_2_PRIVATE}", headers=self.headers_default())
        self.assertEqual(response_403.status_code, 403)
        self.assertNotIn("name", response_403.json())

        response_200 = self.client.get(f"/workflow/{WORKFLOW_3_READ_SHARED}", headers=self.headers_default())
        self.assertEqual(response_200.status_code, 200)
        self.assertEqual(response_200.json(), {
            "name": "Adrianne's Read-Shared Workflow",
            "owner_id": USER_2_ADMIN,
            "description": "",
            "definition": {},
            "is_published": False
        })