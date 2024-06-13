import os
from pathlib import Path
import unittest

from fastapi.testclient import TestClient
from sqlalchemy import Engine, create_engine
from sqlmodel import Session

from ...main import app
from ... import database


SEED_DATA_PATH: Path = Path(__file__).parent.parent.parent.joinpath("db_seed.yaml")


class DbTestCase(unittest.TestCase):
    engine: Engine
    client: TestClient

    token_admin: str
    token_default: str

    def setUp(self) -> None:
        url = os.getenv("MACWORP_TEST_DB_URL")
        if (url is None):
            raise RuntimeError("MACWORP_TEST_DB_URL environment variable not set")

        self.engine = create_engine(url, echo=True)
        
        with Session(self.engine) as session:
            database.seed(session, SEED_DATA_PATH, True)
            session.commit()
        
        self.client = TestClient(app)

        # TODO
        self.token_admin = "admintoken"
        self.token_default = "defaulttoken"

    
    def headers_default(self):
        return {
            "X-Token": self.token_default
        }
    

    def headers_admin(self):
        return {
            "X-Token": self.token_admin
        }

    
    def tearDown(self) -> None:
        self.client.close()
        self.engine.dispose()