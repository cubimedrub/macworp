from collections import defaultdict
from os import error
from flask import request, jsonify

from nf_cloud_backend import app, get_database_connection
from nf_cloud_backend.models.workflow import Workflow

class WorkflowsController:
    @staticmethod
    @app.route("/api/workflows")
    def index():
        offset = request.args.get("offset", None)
        limit = request.args.get("limit", None)
        database_connection = get_database_connection()
        with database_connection.cursor() as database_cursor:
            return jsonify({
                "workflows": [workflow.to_dict() for workflow in Workflow.select(database_cursor, "", [], offset=offset, limit=limit, fetchall=True)]
            })

    @staticmethod
    @app.route("/api/workflows/<int:id>")
    def show(id: int):
        database_connection = get_database_connection()
        with database_connection.cursor() as database_cursor:
            workflow = Workflow.select(database_cursor, "id = %s", [id], fetchall=False)
            if workflow:
                return jsonify({
                    "workflow": workflow.to_dict()
                })
            else:
                return jsonify({}), 404

    @staticmethod
    @app.route("/api/workflows/create", methods=["POST"]) 
    def create():
        errors = defaultdict(list)
        error_status_code = 422

        data = request.json
        name = data.get("name", None)
        if not name == None:
            if len(name) < 1:
                errors["name"].append("to short")
            if len(name) > 512:
                errors["name"].append("to long")
        else:
            errors["name"].append("missing")

        if not len(errors):
            workflow = Workflow(None, name)
            database_connection = get_database_connection()
            with database_connection:
                with database_connection.cursor() as database_cursor:
                    if workflow.insert(database_cursor):
                        return jsonify(workflow.to_dict())
                    else: 
                        raise RuntimeError("workflow insertion returned false ")

        return jsonify({
                "errors": errors
            }), error_status_code

    @staticmethod
    @app.route("/api/workflows/<int:id>/delete", methods=["POST"])
    def delete(id: int):
        database_connection = get_database_connection()
        with database_connection:
            with database_connection.cursor() as database_cursor:
                workflow = Workflow.select(database_cursor, "id = %s", [id], fetchall=False)
                if workflow:
                    if workflow.delete(database_cursor):
                        return jsonify({})
                    else:
                        return jsonify({
                            "errors": {
                                "general": "can not delete workflow, try again later."
                            }
                        }), 422
                else:
                    return jsonify({}), 404

    @staticmethod
    @app.route("/api/workflows/count") 
    def count():
        database_connection = get_database_connection()
        with database_connection.cursor() as database_cursor:
            return jsonify({
                "count": Workflow.count(database_cursor)
            })
    
    @staticmethod
    @app.route("/api/workflows/<int:id>/upload-file", methods=["POST"])
    def upload_file(id: int):
        if "file" in request.files:
            new_file = request.files["file"]
        else:
            return jsonify({
                "errors": {
                    "file": "is missing"
                }
            })
        
        database_connection = get_database_connection()
        with database_connection.cursor() as database_cursor:
            workflow = Workflow.select(database_cursor, "id = %s", [id], fetchall=False)
            workflow.add_file(new_file.filename, new_file.read())
            return jsonify({
                "file": new_file.filename
            })
    
    @staticmethod
    @app.route("/api/workflows/<int:id>/delete-file", methods=["POST"])
    def delete_file(id: int):
        errors = defaultdict(list)
        data = request.json

        filename = data.get("filename", None)
        if filename == None:
            errors["filename"].append("missing")
        elif not isinstance(filename, str):
            errors["filename"].append("not a string")

        if len(errors):
            return jsonify({
                "errors": errors
            }), 422
        
        database_connection = get_database_connection()
        with database_connection.cursor() as database_cursor:
            workflow = Workflow.select(database_cursor, "id = %s", [id], fetchall=False)
            workflow.remove_file(filename)
            return jsonify({
                "file": filename
            })
