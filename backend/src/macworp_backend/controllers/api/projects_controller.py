# std imports
from collections import defaultdict
import json
from pathlib import Path
from typing import Optional
from urllib.parse import unquote

# 3rd party imports
import pandas as pd
from flask import make_response, request, jsonify, send_file, Response
from flask_login import login_required
import pika
import zipstream

# internal imports
from macworp_backend import app, socketio, db_wrapper as db
from macworp_backend.models.project import Project
from macworp_backend.utility.configuration import Configuration
from macworp_backend.errors.unknown_table_format import UnknownTableFormat


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
        offset = request.args.get("offset", 0, type=int)
        limit = request.args.get("limit", 50, type=int)
        if offset < 0:
            offset = 0
        if limit < 1:
            limit = 1
        return jsonify(
            {
                "projects": [
                    project.to_dict()
                    for project in Project.select()
                    .order_by(Project.id.desc())
                    .offset(offset)
                    .limit(limit)
                ]
            }
        )

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
        project: Optional[Project] = Project.get_or_none(Project.id == id)
        if project:
            return jsonify({"project": project.to_dict()})
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

        return jsonify({"errors": errors}), error_status_code

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

        for key in ["workflow_id", "workflow_arguments"]:
            if not key in data:
                errors[key].append("can not be empty")

        if not isinstance(data["workflow_id"], int):
            errors["workflow_id"].append("must be integer")

        if not isinstance(data["workflow_arguments"], dict):
            errors["workflow_arguments"].append("must be a dictionary")

        if len(errors):
            jsonify({"errors": errors})
        project: Optional[Project] = Project.get_or_none(Project.id == id)
        if project:
            project.workflow_arguments = data["workflow_arguments"]
            project.workflow_id = data["workflow_id"]  # TODO save id
            project.save()
            return jsonify({}), 200
        else:
            return jsonify({}), 404

    @staticmethod
    @app.route("/api/projects/<int:id>/delete", methods=["POST"])
    @login_required
    def delete(id: int):
        project: Optional[Project] = Project.get_or_none(Project.id == id)
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
        return jsonify({"count": Project.select().count()})

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
        project: Optional[Project] = Project.get_or_none(Project.id == id)
        if project is None:
            return jsonify({"errors": {"general": "project not found"}})
        directory = project.get_path(
            Path(unquote(request.args.get("dir", "/", type=str)))
        )
        if directory.is_dir() and project.in_file_directory(directory):
            files = []
            folders = []
            for entry in directory.iterdir():
                if entry.is_dir():
                    folders.append(entry.name)
                else:
                    files.append(entry.name)

            folders.sort()
            files.sort()
            return jsonify({"folders": folders, "files": files})
        return jsonify({"errors": {"general": "directory not found"}}), 404

    @staticmethod
    @app.route("/api/projects/<int:id>/upload-file", methods=["POST"])
    @login_required
    def upload_file(id: int):
        """
        Upload files to the project directory.
        Use `/api/projects/<int:id>/upload-file-chunk` for large files.

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

        if not "file_path" in request.files:
            errors["file_path"].append("cannot be empty")

        if len(errors) > 0:
            return jsonify({"errors": errors}), 422

        file_path = Path(request.files["file_path"].read().decode("utf-8"))

        project: Optional[Project] = Project.get_or_none(Project.id == id)
        if project is None:
            return "", 404

        file_path = project.add_file(file_path, request.files["file"].read())
        return jsonify({"file_path": str(file_path)})

    @staticmethod
    @app.route("/api/projects/<int:project_id>/upload-file-chunk", methods=["POST"])
    @login_required
    def upload_file_chunk(project_id: int):
        """
        Upload file chunks to the project directory. Useful for uploading large files.

        Method
        ------
        POST

        Query parameters
        ----------------
        is-dropzone : int
            > 0 if the request is from Dropzone.js to set the correct chunk offset in the request

        Form data
        ---------
        file : File
            File chunk
        file_path : File
            Path of the file within the project directory
        chunk-offset : int
            Byte offset of the file chunk (Dropzone.js is sending it as `dzchunkbyteoffset`)


        Parameters
        ----------
        project_id : int
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

        if not "file_path" in request.files:
            errors["file_path"].append("cannot be empty")

        if len(errors) > 0:
            return jsonify({"errors": errors}), 422

        file_path = Path(request.files["file_path"].read().decode("utf-8"))

        chunk_offset_key = (
            "dzchunkbyteoffset"
            if request.args.get("is-dropzone", 0, type=bool)
            else "chunk-offset"
        )
        chunk_offset = request.form.get(chunk_offset_key, 0, type=int)

        project: Optional[Project] = Project.get_or_none(Project.id == project_id)
        if project is None:
            return "", 404

        file_path = project.add_file_chunk(
            file_path, chunk_offset, request.files["file"].stream
        )
        return jsonify({"file_path": str(file_path)})

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
            return jsonify({"errors": errors}), 422

        project: Optional[Project] = Project.get_or_none(Project.id == id)
        if project is None:
            return "", 404
        project.remove_path(Path(path))
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

        path = data.get("path", None)
        if path is None:
            errors["path"].append("missing")
        elif not isinstance(path, str):
            errors["target_path"].append("not a string")

        if len(errors):
            return jsonify({"errors": errors}), 422

        project: Optional[Project] = Project.get_or_none(Project.id == id)
        if project is None:
            return "", 404
        project.create_folder(Path(path))
        return "", 200

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
            409 - project is already in ignore state
            422 - errors
        """
        errors = defaultdict(list)
        project: Optional[Project] = Project.get_or_none(Project.id == id)
        if project is None:
            return "", 404
        if project.ignore:
            return (
                jsonify(
                    {
                        "errors": {
                            "general": "project is ignored and cannot be scheduled"
                        }
                    }
                ),
                409,
            )
        if project and not project.is_scheduled:
            for arguments in project.workflow_arguments:
                if arguments["type"] == "separator":
                    continue
                if (
                    not "value" in arguments
                    or "value" in arguments
                    and arguments["value"] is None
                ):
                    errors[arguments["label"]].append("cannot be empty")
            if len(errors) > 0:
                return jsonify({"errors": errors}), 422
            with db.database.atomic() as transaction:
                project.is_scheduled = True
                project.save()
                try:
                    connection = pika.BlockingConnection(
                        pika.URLParameters(Configuration.values()["rabbit_mq"]["url"])
                    )
                    channel = connection.channel()
                    channel.basic_publish(
                        exchange="",
                        routing_key=Configuration.values()["rabbit_mq"][
                            "project_workflow_queue"
                        ],
                        body=project.get_queue_representation()
                        .model_dump_json()
                        .encode(),
                    )
                    connection.close()
                except BaseException as exception:
                    transaction.rollback()
                    raise exception
                return jsonify({"is_scheduled": project.is_scheduled})
        else:
            return jsonify({"errors": {"general": "project not found"}}), 422

    @staticmethod
    @app.route("/api/projects/<int:project_id>/is-ignored", methods=["GET"])
    @login_required
    def is_ignored(project_id: int):
        """
        Endpoint to quickly check if project is currently ignored.

        Parameters
        ----------
        project_id : int
            Project ID

        Returns
        -------
        Response
            * 200 - Project is ignored
            * 204 - If project is not ignored
            * 404 - If project was not found
        """
        project: Optional[Project] = Project.get_or_none(Project.id == project_id)
        if project is None:
            return "", 404
        return "", 200 if project.ignore else 204

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
        project: Optional[Project] = Project.get_or_none(Project.id == id)
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
            return jsonify({"errors": errors}), 422
        project: Optional[Project] = Project.get_or_none(Project.id == id)
        if project is None:
            return jsonify({"errors": {"general": "project not found"}}), 404
        # Handle process traces
        if "trace" in workflow_log and "event" in workflow_log:
            match workflow_log["event"]:
                case "process_submitted":
                    project.submitted_processes += 1
                case "process_completed":
                    project.completed_processes += 1
            project.save()
            socketio.emit(
                "new-progress",
                {
                    "submitted_processes": project.submitted_processes,
                    "completed_processes": project.completed_processes,
                    "details": f"Task {workflow_log['trace']['task_id']}: {workflow_log['trace']['name']} - {workflow_log['trace']['status']}",
                },
                to=f"project{project.id}",
            )
        # Handle workflow traces
        elif "metadata" in workflow_log and "event" in workflow_log:
            error_report: Optional[str] = workflow_log["metadata"]["workflow"].get(
                "errorReport", None
            )
            if error_report is not None:
                socketio.emit(
                    "error", {"error_report": error_report}, to=f"project{project.id}"
                )
        return "", 200

    @staticmethod
    @app.route("/api/projects/<int:project_id>/download")
    @login_required
    def download(project_id: int):
        """
        Downloads a file or folder.
        If path is a folder the response is a zip package.

        Parameters
        ----------
        project_id : int
            Project ID
        path : strs
            Path to folder or file.

        URL query parameters
        --------------------
        path : str
            Path to download
        is-inline : int
            If true, the download is inline (for further processing),
            otherwise as attachment (browser download). Values: 0 == False, >0 == True, default: False
            (only valid for files)
        with-metadata : int
            If true, the file metadata will be delivered as headers (`MMData-Header` & `MMData-Description`).
            Values: 0 == False, >0 == True, default: False
            (only valid for files)
        is-table: : int
            If true, the table-file will be returned as JSON table, see: Pandas documentation `DataFrame.to_json(orient="split")`
            Values: 0 == False, >0 == True, default: False
        """
        # Get project
        project: Optional[Project] = Project.get_or_none(Project.id == project_id)
        if project is None:
            return jsonify({"errors": {"project": ["not found"]}}), 404

        # Set path
        path = Path(unquote(request.args.get("path", "", type=str)))
        path_to_download = project.get_path(path)

        # Set download type
        is_inline: bool = request.args.get("is-inline", False, type=bool)

        # Set if metadata is added
        with_metadata: bool = request.args.get("with-metadata", False, type=bool)

        # Set if table is requested
        is_table: bool = request.args.get("is-table", False, type=bool)

        if path_to_download.is_file():
            response: Optional[Response] = None
            if not is_table:
                response = ProjectsController.file_download(path_to_download, is_inline)
            else:
                try:
                    response = ProjectsController.table_download(path_to_download)
                except UnknownTableFormat as error:
                    return (
                        jsonify(
                            {
                                "errors": {
                                    "general": f"unknown table format, supported types are: {', '.join(error.supported_table_formats)}",
                                }
                            }
                        ),
                        422,
                    )

            if with_metadata:
                metadata_file_path = path_to_download.with_suffix(
                    f"{path_to_download.suffix}.mmdata"
                )
                metadata = {}
                if metadata_file_path.is_file():
                    metadata = json.loads(metadata_file_path.read_text())
                response.headers["MMD-Header"] = metadata.get("header", "")
                response.headers["MMD-Description"] = metadata.get("description", "")

            return response

        elif path_to_download.is_dir():

            def build_stream():
                stream = zipstream.ZipFile(mode="w", compression=zipstream.ZIP_DEFLATED)
                project_path_len = len(str(project.file_directory))
                for path in path_to_download.glob("**/*"):
                    if path.is_file():
                        stream.write(
                            path, arcname=str(path)[project_path_len:]
                        )  # Remove absolut path before project subfolder, including project id
                yield from stream

            response = Response(build_stream(), mimetype="application/zip")
            filename_friendly_path_as_str = str(path).replace("/", "+")
            response.headers["Content-Disposition"] = (
                f"attachment; filename={project.name}--{filename_friendly_path_as_str}.zip"
            )
            return response
        else:
            return jsonify({"errors": {"path": ["not found"]}}), 404

    @staticmethod
    def file_download(path: Path, is_inline: bool) -> Response:
        """Downloads a file inline or as attatchment

        Parameters
        ----------
        path : Path
            Full path to file crated with `Project.get_path()`
        is_inline : bool
            If true, file is downloaded inline, otherwise as attachment

        Returns
        -------
        Response
            Flask response with file
        """
        return make_response(send_file(path, as_attachment=not is_inline))

    @staticmethod
    def table_download(path: Path) -> Response:
        """Downloads table as JSON, created with Pandas `DataFrame.to_json(orient="split")`, e.g.
        ```json
        {
            "columns": ["col1", "col2", ...],
            "data": [
                ["row1col1", "row1col2", ...],
                ["row2col1", "row2col2", ...],
                ...
            ]
        }
        ```
        Supported are CSV, TSV and XLSX files.

        Parameters
        ----------
        path : Path
            Full path to file crated with `Project.get_path()`

        Returns
        -------
        Response
            Response with table data
        """
        dataframe: Optional[pd.DataFrame] = None
        try:
            match path.suffix.lower():
                case ".csv":
                    dataframe = pd.read_csv(path)
                case ".tsv":
                    dataframe = pd.read_csv(path, sep="\t")
                case ".xlsx":
                    dataframe = pd.read_excel(path)
        except pd.errors.EmptyDataError:
            dataframe = pd.DataFrame()

        if dataframe is None:
            raise UnknownTableFormat(["CSV", "TSV", "XLSX"])

        return Response(
            dataframe.to_json(orient="split", index=False), mimetype="application/json"
        )

    @staticmethod
    @app.route("/api/projects/<int:project_id>/file-size")
    @login_required
    def file_size(project_id: int):
        """
        Returns the size of a file

        Parameters
        ----------
        project_id : int
            Project ID

        URL query parameters
        --------------------
        path : str
            Path to file

        Returns
        -------
        Response
            * 200 - on success, JSON with `size` keys, value is the size in bytes
            * 404 - on project or path not found
            * 422 - on path is not a file

        """
        project: Optional[Project] = Project.get_or_none(Project.id == project_id)
        if project is None:
            return jsonify({"errors": {"general": ["project not found"]}}), 404
        path = Path(unquote(request.args.get("path", "", type=str)))
        path_to_download = project.get_path(path)
        if not path_to_download.exists():
            return jsonify({"errors": {"path": ["not found"]}}), 404
        if not path_to_download.is_file():
            return jsonify({"errors": {"path": ["not a file"]}}), 422

        return jsonify({"size": path_to_download.stat().st_size})

    @staticmethod
    @app.route("/api/projects/<int:project_id>/metadata")
    @login_required
    def metadata(project_id: int):
        """
        Returns the metadata of a file

        Parameters
        ----------
        project_id : int
            Project ID

        URL query parameters
        --------------------
        path : str
            Path to file

        Returns
        -------
        Response
            * 200 - on success, JSON with header and description
            * 404 - on project or path not found
            * 422 - on metadata is not a file

        """
        project: Optional[Project] = Project.get_or_none(Project.id == project_id)
        if project is None:
            return jsonify({"errors": {"general": ["project not found"]}}), 404
        path = Path(unquote(request.args.get("path", "", type=str)))
        path_to_download = project.get_path(path)
        metadata_file_path = path_to_download.with_suffix(
            f"{path_to_download.suffix}.mmdata"
        )
        if not metadata_file_path.exists():
            return jsonify({"errors": {"path": ["not found"]}}), 404
        if not metadata_file_path.is_file():
            return jsonify({"errors": {"path": ["not a file"]}}), 422

        return Response(metadata_file_path.read_text(), content_type="application/json")
