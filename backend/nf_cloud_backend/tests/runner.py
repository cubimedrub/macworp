import csv
import importlib
import inspect
import os
from pathlib import Path
import pkgutil
from unittest import TestCase, TestLoader, TextTestRunner

from fastapi.testclient import TestClient
from sqlmodel import SQLModel, Session, create_engine

from ..main import app, engine
from .. import database
from .base import Client, LogEntry, Test
from . import cases
from ..models.prelude import *


def create_test_function(test: type[Test], log_requests: Path | None):
    def run_test(self: TestCase):
        seed_data_path = os.getenv("MACWORP_TEST_SEED_DATA")
        if (seed_data_path is None):
            raise RuntimeError("MACWORP_TEST_SEED_DATA environment variable not set")
        
        SQLModel.metadata.drop_all(engine)
        SQLModel.metadata.create_all(engine)

        with Session(engine) as session:
            database.seed(session, Path(seed_data_path), True)
            session.commit()
        
        test_client = TestClient(app)
        client = Client(test_client, self)

        exception = None
        try:
            test.run(client)
        except Exception as e:
            exception = e
            raise e
        finally:
            if isinstance(log_requests, Path):
                update_logs(test, client.logs, log_requests, "SUCCEEDED" if exception is None else "FAILED")
            test_client.close()
            SQLModel.metadata.drop_all(engine)
    return run_test


def update_logs(test: type[Test], logs: list[LogEntry], path: Path, status: str):
    file_exists = os.path.isfile(path)
    with open(path, mode="a") as file:
        writer = csv.writer(file)
        if not file_exists:
            writer.writerow(["Name", "Description", "Test Status", "Call Number", "Method", "URL", "Body", "Headers", "Response JSON", "Response Status"])
        writer.writerow([
            test.name,
            test.description,
            status,
            "",
            "",
            "",
            "",
            "",
            "",
            "",
        ])
        for i, log in enumerate(logs):
            writer.writerow([
                "",
                "",
                "",
                i + 1,
                log.request.method,
                log.request.url,
                log.request.content.decode("utf-8"),
                log.request.headers,
                log.json,
                log.status
            ])


def run_tests(log_requests: Path | None):
    if log_requests is not None:
        log_requests.unlink(missing_ok=True)

    GeneratedTests = type("GeneratedTests", (TestCase,), {})

    for importer, modname, ispkg in pkgutil.walk_packages(cases.__path__, prefix=cases.__name__ + '.'):
        module = importlib.import_module(modname)
        for name, test in inspect.getmembers(module, inspect.isclass):
            if issubclass(test, Test) and test is not Test:
                setattr(GeneratedTests, f"test_{test.__name__}", create_test_function(test, log_requests))

    suite = TestLoader().loadTestsFromTestCase(GeneratedTests)

    runner = TextTestRunner()
    runner.run(suite)