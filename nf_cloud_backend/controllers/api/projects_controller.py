# std imports
from collections import defaultdict
import json
from urllib.parse import unquote

# 3rd party imports
from flask import request, jsonify, send_file, Response
from flask_login import login_required
import pika
import zipstream

# internal imports
from nf_cloud_backend import app, socketio, db_wrapper as db
from nf_cloud_backend.models.project import Project
from nf_cloud_backend.utility.configuration import Configuration

class ProjectsController:
    """
    Controller for project endpoints.
    """

    @staticmethod
    @app.route("/api/projects")
    @login_required
    def index():
        """
        Endpoint for listing all project.

        Returns
        -------
        Response
        """
        offset = request.args.get("offset", None)
        limit = request.args.get("limit", None)
        return jsonify({
            "projects": [
                project.to_dict() 
                for project in Project.select().offset(offset).limit(limit)
            ]
        })

    @staticmethod
    @app.route("/api/projects/<int:id>")
    @login_required
    def show(id: int):
        """
        Endpoint for requesting project attributes.

        Parameters
        ----------
        id : int
            Project ID

        Returns
        -------
        Response
        """
        project = Project.get(Project.id == id)
        if project:
            return jsonify({
                "project": project.to_dict()
            })
        else:
            return jsonify({}), 404

    @staticmethod
    @app.route("/api/projects/create", methods=["POST"])
    @login_required
    def create():
        """
        Endpoint for creating a project.

        Returns
        -------
        Response
            200 - on success
            422 - on errors

        Raises
        ------
        RuntimeError
            Project cannot be inserted.
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
            project = Project.create(name=name)
            return jsonify(project.to_dict())

        return jsonify({
                "errors": errors
            }), error_status_code

    @staticmethod
    @app.route("/api/projects/<int:id>/update", methods=["POST"])
    @login_required
    def update(id: int):
        """
        Endpoint for updating project.

        Parameters
        ----------
        id : int
            Project ID

        Returns
        -------
        Response
            200 - on success
            404 - when ressource not found
            422 - on error
        """
        errors = defaultdict(list)
        data = request.json

        for key in ["workflow", "workflow_arguments"]:
            if not key in data:
                errors[key].append("can not be empty")
        
        if not isinstance(data["workflow"], str):
                errors["workflow"].append("must be string")

        if not isinstance(data["workflow_arguments"], dict):
            errors["workflow_arguments"].append("must be a dictionary")

        if len(errors):
            jsonify({
                "errors": errors
            })
        project = Project.get(Project.id == id)
        if project:
            project.workflow_arguments = data["workflow_arguments"]
            project.workflow = data["workflow"]
            project.save()
            return jsonify({}), 200
        else:
            return jsonify({}), 404

    @staticmethod
    @app.route("/api/projects/<int:id>/delete", methods=["POST"])
    @login_required
    def delete(id: int):
        project = Project.get(Project.id == id)
        if project:
            project.delete_instance()
            return jsonify({})
        else:
            return jsonify({}), 404

    @staticmethod
    @app.route("/api/projects/count") 
    def count():
        """
        Returns number of project

        Returns
        -------
        Response
            200
        """
        return jsonify({
            "count": Project.select().count()
        })
    

    @staticmethod
    @app.route("/api/projects/<int:id>/files")
    @login_required
    def files(id: int):
        """
        List files

        Parameters
        ----------
        id : int
            Project ID

        Returns
        -------
        Reponse
            200 - on success
            404 - when project was not found
        """
        project = Project.get(Project.id == id)
        if project is None:
            return jsonify({
                "errors": {
                    "general": "project not found"
                }
            })
        directory = project.get_path(
            unquote(request.args.get('dir', "", type=str))
        )
        if directory.is_dir() and project.in_file_director:
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
    @app.route("/api/projects/<int:id>/upload-file", methods=["POST"])
    @login_required
    def upload_file(id: int):
        """
        Upload files

        Parameters
        ----------
        id : int
            Project ID

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

        project = Project.get(Project.id == id)
        if project is None:
            return "", 404
        project.add_file(directory, new_file.filename, new_file.read())
        return jsonify({
            "directory": directory,
            "file": new_file.filename
        })
    
    @staticmethod
    @app.route("/api/projects/<int:id>/delete-path", methods=["POST"])
    @login_required
    def delete_path(id: int):
        """
        Deletes a path.

        Parameters
        ----------
        id : int
            Project id

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
        
        project = Project.get(Project.id == id)
        if project is None:
            return "", 404
        project.remove_path(path)
        return "", 200

    @staticmethod
    @app.route("/api/projects/<int:id>/create-folder", methods=["POST"])
    @login_required
    def create_folder(id: int):
        """
        Creates the given path within the project directory

        Parameters
        ----------
        id : int
            Project ID

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

        project = Project.get(Project.id == id)
        if project is None:
            return "", 404
        project.create_folder(target_path, new_path)
        return jsonify({}), 200

    @staticmethod
    @app.route("/api/projects/<int:id>/schedule", methods=["POST"])
    @login_required
    def schedule(id: int):
        """
        Endpoint to schedule project for execution in RabbitMQ.

        Parameters
        ----------
        id : int
            Project ID

        Returns
        -------
        Response
            200 - successfull
            422 - errors
        """
        errors = defaultdict(list)
        project = Project.get(Project.id == id)
        if project is None:
            return "", 404
        if project and not project.is_scheduled:
            for arg_name, arg_definition in project.workflow_arguments.items():
                if not "value" in arg_definition or "value" in arg_definition and arg_definition["value"] is None:
                    errors[arg_name].append("cannot be empty")
            if len(errors) > 0:
                return jsonify({
                    "errors": errors
                }), 422
            with db.database.atomic() as transaction:
                project.is_scheduled = True
                project.save()
                try:
                    connection = pika.BlockingConnection(pika.URLParameters(Configuration.values()['rabbit_mq']['url']))
                    channel = connection.channel()
                    channel.basic_publish(exchange='', routing_key=Configuration.values()['rabbit_mq']['project_workflow_queue'], body=project.get_queue_represenation())
                    connection.close()
                except BaseException as exception:
                    transaction.rollback()
                    raise exception
                return jsonify({
                    "is_scheduled": project.is_scheduled
                })
        else:
            return jsonify({
                    "errors": {
                        "general": "project not found"
                    }
                }), 422

    @staticmethod
    @app.route("/api/projects/<int:id>/finished", methods=["POST"])
    @login_required
    def finished(id: int):
        """
        Endpoint to finialize set project as finished by the worker.

        Parameters
        ----------
        id : int
            ID of project

        Return
        ------
        Response
            200 - Empty response
        """
        project = Project.get(Project.id == id)
        if project is None:
            return "", 404
        project.is_scheduled = False
        project.submitted_processes = 0
        project.completed_processes = 0
        project.save()
        socketio.emit("finished-project", {}, to=f"project{project.id}")
        return "", 200

    @staticmethod
    @app.route("/api/projects/<int:id>/workflow-log", methods=["POST"])
    @login_required
    def workflow_log(id: int):
        """
        Endpoint for Nextflow to report log.
        If log is received, a event is send to the browser with submitted and completed processes.

        Parameters
        ----------
        id : int
            ID of project

        Returns
        -------
        Response
            200 - emtpy, on success
            422 - on errors
        """
        errors = defaultdict(list)
        workflow_log = json.loads(request.data.decode("utf-8"))

        if workflow_log is None:
            errors["request body"].append("cannot be empty")
        elif not isinstance(workflow_log, dict):
            errors["request body"].append("must be a string")

        if len(errors):
            return jsonify({
                "errors": errors
            }), 422

        if "event" in workflow_log and "trace" in workflow_log:
            project = Project.get(Project.id == id)
            if project is None:
                return jsonify({
                    "errors": {
                        "general": "project not found"
                    }
                }),  404
            if workflow_log["event"] == "process_submitted":
                project.submitted_processes += 1
            if workflow_log["event"] == "process_completed":
                project.completed_processes += 1
            project.save()
            socketio.emit(
                "new-progress", 
                {
                    "submitted_processes": project.submitted_processes,
                    "completed_processes": project.completed_processes
                }, 
                to=f"project{project.id}"
            )
        return "", 200

    @staticmethod
    @app.route("/api/projects/<int:w_id>/download")
    @login_required
    def download(w_id: int):
        """
        Downloads a file or folder.
        If path is a folder the response is a zip package.

        Parameters
        ----------
        w_id : int
            Project ID
        path : strs
            Path to folder or file.
        """
        project = Project.get(Project.id == w_id)
        if project is None:
            return "", 404
        path = unquote(request.args.get('path', "", type=str))
        path_to_download = project.get_path(path)
        if not path_to_download.exists():
            return "", 404
        elif path_to_download.is_file():
            return send_file(path_to_download, as_attachment=True)
        else:
            def build_stream():
                    stream = zipstream.ZipFile(mode='w', compression=zipstream.ZIP_DEFLATED)
                    project_path_len = len(str(project.file_directory))
                    for path in path_to_download.glob("**/*"):
                        if path.is_file():
                            stream.write(path, arcname=str(path)[project_path_len:])    # Remove absolut path before project subfolder, including project id
                    yield from stream
            response = Response(build_stream(), mimetype='application/zip')
            response.headers["Content-Disposition"] = f"attachment; filename={project.name}--{path.replace('/', '+')}.zip"
            return response


