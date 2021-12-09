import json
import pika
from collections import defaultdict
from flask import request, jsonify

from nf_cloud_backend import app, get_database_connection, config, socketio
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
    @app.route("/api/workflows/<int:id>/update", methods=["POST"])
    def update(id: int):
        errors = defaultdict(list)
        data = request.json

        for key in ["nextflow_workflow", "nextflow_workflow", "nextflow_arguments"]:
            if not key in data:
                errors[key].append("can not be empty")
            elif not isinstance(data[key], str):
                errors[key].append("must be string")

        if len(errors):
            jsonify({
                "errors": errors
            })

        database_connection = get_database_connection()
        with database_connection:
            with database_connection.cursor() as database_cursor:
                workflow = Workflow.select(database_cursor, "id = %s", [id], fetchall=False)
                if workflow:
                    workflow.nextflow_arguments = data["nextflow_arguments"]
                    workflow.nextflow_workflow = data["nextflow_workflow"]
                    workflow.nextflow_workflow_type = data["nextflow_workflow_type"]
                    if workflow.update(database_cursor):
                        return jsonify({}), 200
                    else:
                        return jsonify({}), 422
                else:
                    return jsonify({}), 404



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

    @staticmethod
    @app.route("/api/workflows/<int:id>/schedule", methods=["POST"])
    def schedule(id: int):
        errors = defaultdict(list)
        database_connection = get_database_connection()
        with database_connection:
            with database_connection.cursor() as database_cursor:
                workflow = Workflow.select(database_cursor, "id = %s", [id], fetchall=False)
                if workflow and not workflow.is_scheduled:
                    for arg_name, arg_definition in workflow.nextflow_arguments.items():
                        if not "value" in arg_definition or "value" in arg_definition and arg_definition["value"] is None:
                            errors[arg_name].append("cannot be empty")
                    if len(errors) > 0:
                        return jsonify({
                            "errors": errors
                        }), 422
                    workflow.is_scheduled = True
                    workflow.update(database_cursor)
                    connection = pika.BlockingConnection(pika.URLParameters(config['rabbit_mq']['url']))
                    channel = connection.channel()
                    channel.basic_publish(exchange='', routing_key=config['rabbit_mq']['workflow_queue'], body=workflow.get_queue_represenation())
                    connection.close()
                    return jsonify({
                        "is_scheduled": workflow.is_scheduled
                    })
                else:
                    return jsonify({
                            "errors": {
                                "general": "workflow not found"
                            }
                        }), 422

    @staticmethod
    @app.route("/api/workflows/<int:id>/finished", methods=["POST"])
    def finished(id: int):
        database_connection = get_database_connection()
        with database_connection:
            with database_connection.cursor() as database_cursor:
                workflow = Workflow.select(database_cursor, "id = %s", [id], fetchall=False)
                workflow.is_scheduled = False
                workflow.submitted_processes = 0
                workflow.completed_processes = 0
                workflow.update(database_cursor)
                socketio.emit("finished-workflow", {}, to=f"workflow{workflow.id}")
                return "", 200

    @staticmethod
    @app.route("/api/workflows/<int:id>/nextflow-log", methods=["POST"])
    def nextflow_log(id: int):
        errors = defaultdict(list)
        nextflow_log = json.loads(request.data.decode("utf-8"))

        if nextflow_log is None:
            errors["request body"].append("cannot be empty")
        elif not isinstance(nextflow_log, dict):
            errors["request body"].append("must be a string")

        if len(errors):
            return jsonify({
                "errors": errors
            }), 422

        if "event" in nextflow_log and "trace" in nextflow_log:
            database_connection = get_database_connection()
            with database_connection:
                with database_connection.cursor() as database_cursor:
                    workflow = Workflow.select(database_cursor, "id = %s", [id], fetchall=False)
                    if nextflow_log["event"] == "process_submitted":
                        workflow.submitted_processes += 1
                    if nextflow_log["event"] == "process_completed":
                        workflow.completed_processes += 1
                    workflow.update(database_cursor)
                    socketio.emit(
                        "new-progress", 
                        {
                            "submitted_processes": workflow.submitted_processes,
                            "completed_processes": workflow.completed_processes
                        }, 
                        to=f"workflow{workflow.id}"
                    )
        
        return "", 200
