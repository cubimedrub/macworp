# std imports
from collections import defaultdict
import json
from urllib.parse import unquote

# 3rd party imports
from flask import request, jsonify, send_file, Response
import pika


# internal imports
from nf_cloud_backend import app, get_database_connection, config, socketio
from nf_cloud_backend.models.workflow import Workflow

class WorkflowsController:
    """
    Controller for workflow endpoints.
    """

    @staticmethod
    @app.route("/api/workflows")
    def index():
        """
        Endpoint for listing all workflows.

        Returns
        -------
        Response
        """
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
        """
        Endpoint for requesting workflow attributes.

        Parameters
        ----------
        id : int
            Workflow ID

        Returns
        -------
        Response
        """
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
        """
        Endpoint for creating a workflow.

        Returns
        -------
        Response
            200 - on success
            422 - on errors

        Raises
        ------
        RuntimeError
            Workflow cannot be inserted.
        """
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
        """
        Endpoint for updating workflow.

        Parameters
        ----------
        id : int
            Workflow ID

        Returns
        -------
        Response
            200 - on success
            404 - when ressource not found
            422 - on error
        """
        errors = defaultdict(list)
        data = request.json

        for key in ["nextflow_workflow", "nextflow_arguments"]:
            if not key in data:
                errors[key].append("can not be empty")
        
        if not isinstance(data["nextflow_workflow"], str):
                errors["nextflow_workflow"].append("must be string")

        if not isinstance(data["nextflow_arguments"], dict):
            errors["nextflow_arguments"].append("must be a dictionary")

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
        """
        Returns number of workflows

        Returns
        -------
        Response
            200
        """
        database_connection = get_database_connection()
        with database_connection.cursor() as database_cursor:
            return jsonify({
                "count": Workflow.count(database_cursor)
            })
    

    @staticmethod
    @app.route("/api/workflows/<int:id>/files")
    def files(id: int):
        """
        List files

        Parameters
        ----------
        id : int
            Workflow ID

        Returns
        -------
        Reponse
            200 - on success
            404 - when workflow was not found
        """
        directory = unquote(request.args.get('dir', "", type=str))
        database_connection = get_database_connection()
        with database_connection.cursor() as database_cursor:
            workflow = Workflow.select(database_cursor, "id = %s", [id], fetchall=False)
            directory = workflow.get_path(directory)
            if directory.is_dir() and workflow.in_file_director:
                files = []
                folders = []
                for entry in directory.iterdir():
                    if entry.is_dir():
                        folders.append(entry.name)
                    else:
                        files.append(entry.name)

                folders.sort()
                files.sort()
                return jsonify({
                    "folders": folders,
                    "files": files
                })
        return jsonify({
            "errors": {
                "general": "directory not found"
            }
        }), 404


    @staticmethod
    @app.route("/api/workflows/<int:id>/upload-file", methods=["POST"])
    def upload_file(id: int):
        """
        Upload files

        Parameters
        ----------
        id : int
            Workflow ID

        Returns
        -------
        Response
            200 - on success
            422 - on error
        """
        errors = defaultdict(list)

        if not "file" in request.files:
            errors["file"].append("cannot be empty")

        directory = request.form.get("directory", None)
        if directory is None:
            errors["directory"].append("cannot be empty")
        elif not isinstance(directory, str):
            errors["directory"].append("is not type string")

        if len(errors) > 0:
            return jsonify({
                "errors": errors
            }), 422
        
        new_file = request.files["file"]

        database_connection = get_database_connection()
        with database_connection.cursor() as database_cursor:
            workflow = Workflow.select(database_cursor, "id = %s", [id], fetchall=False)
            workflow.add_file(directory, new_file.filename, new_file.read())
            return jsonify({
                "directory": directory,
                "file": new_file.filename
            })
    
    @staticmethod
    @app.route("/api/workflows/<int:id>/delete-path", methods=["POST"])
    def delete_path(id: int):
        """
        Deletes a path.

        Parameters
        ----------
        id : int
            Workflow id

        Returns
        -------
        Response
        """
        errors = defaultdict(list)
        data = request.json

        path = data.get("path", None)
        if path == None:
            errors["path"].append("missing")
        elif not isinstance(path, str):
            errors["path"].append("not a string")

        if len(errors):
            return jsonify({
                "errors": errors
            }), 422
        
        database_connection = get_database_connection()
        with database_connection.cursor() as database_cursor:
            workflow = Workflow.select(database_cursor, "id = %s", [id], fetchall=False)
            workflow.remove_path(path)
            return "", 200

    @staticmethod
    @app.route("/api/workflows/<int:id>/create-folder", methods=["POST"])
    def create_folder(id: int):
        """
        Creates the given path within the workflow directory

        Parameters
        ----------
        id : int
            Workflow ID

        Returns
        -------
        Status code 422
            Dictionary with key `errors`, value is a dictionary with parameter names as key and string array with errors
        Status code 200
            Empty if all has worked
        """
        errors = defaultdict(list)
        data = request.json

        target_path = data.get("target_path", None)
        if target_path == None:
            errors["target_path"].append("missing")
        elif not isinstance(target_path, str):
            errors["target_path"].append("not a string")

        new_path = data.get("new_path", None)
        if new_path == None:
            errors["new_path"].append("missing")
        elif not isinstance(new_path, str):
            errors["new_path"].append("not a string")

        if len(errors):
            return jsonify({
                "errors": errors
            }), 422

        database_connection = get_database_connection()
        with database_connection.cursor() as database_cursor:
            workflow = Workflow.select(database_cursor, "id = %s", [id], fetchall=False)
            workflow.create_folder(target_path, new_path)
            return "", 200

    @staticmethod
    @app.route("/api/workflows/<int:id>/schedule", methods=["POST"])
    def schedule(id: int):
        """
        Endpoint to schedule workflow for execution in RabbitMQ.

        Parameters
        ----------
        id : int
            Workflow ID

        Returns
        -------
        Response
            200 - successfull
            422 - errors
        """
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
        """
        Endpoint to finialize set workflow as finished by the worker.

        Parameters
        ----------
        id : int
            ID of workflow

        Return
        ------
        Response
            200 - Empty response
        """
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
        """
        Endpoint for Nextflow to report log.
        If log is received, a event is send to the browser with submitted and completed processes.

        Parameters
        ----------
        id : int
            ID of workflow

        Returns
        -------
        Response
            200 - emtpy, on success
            422 - on errors
        """
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


