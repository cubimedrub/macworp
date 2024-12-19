# std imports
import json
from typing import Any, Callable, Dict, FrozenSet, Iterator, Optional, Tuple

# external imports
from flask import jsonify, request
from peewee import Database
from playhouse.db_url import connect as peewee_connect

# internal imports
from macworp_backend import app
from macworp_backend.utility.configuration import Configuration


class RecordSelectorController:
    PEEWEE_SUPPORT: FrozenSet[str] = frozenset(("apsw", "mysql", "postgres", "sql"))
    """Database protocols supported by peewee
    """

    @staticmethod
    @app.route("/api/record-selectors/:workflow/:argument")
    def select_records(workflow: str, argument: str):
        """
        Returns
        -------
        JSON with key `nextflow_workflows`, which contains a list of workflow names.
        """
        workflow_argument: Dict[str, Any] = Configuration.values()["workflows"][
            workflow
        ]["parameters"]["dynamic"][argument]
        offset: Optional[int] = request.args.get("offset", None, type=int)

        db_url_protocol_idx: int = workflow_argument["database_url"].find("://")
        if db_url_protocol_idx < 0:
            return jsonify({"general": "no protocol found"})
        db_protocol: str = workflow_argument["database_url"][:db_url_protocol_idx]
        if db_protocol in RecordSelectorController.PEEWEE_SUPPORT:
            return RecordSelectorController.select_sql(workflow_argument, offset)

        return jsonify({"general": "unknown database protocol"})

    @staticmethod
    def select_sql(
        workflow_argument: Dict[str, Any], offset: Optional[int]
    ) -> Tuple[Callable, Dict[str, str]]:
        select_statement: str = workflow_argument["select"]
        if offset is not None:
            select_statement = select_statement.replace(":offset:", offset)

        def record_stream() -> Iterator[bytes]:
            yield b'{"records":['
            db_conn: Database = peewee_connect(workflow_argument["database_url"])
            try:
                db_cur = db_conn.execute_sql(select_statement)
                for row in db_cur:
                    yield json.dumps(row).encode("utf-8")
            finally:
                db_conn.close()
            yield b"]}"

        return record_stream(), {"Content-Type": "application/json"}
