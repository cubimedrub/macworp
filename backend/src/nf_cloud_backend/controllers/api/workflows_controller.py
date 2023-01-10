# std imports
from collections import defaultdict
import json
from pathlib import Path
from typing import Any, Dict, Optional, List

#3rd party import
from flask import jsonify, request
import jsonschema

# internal imports
from nf_cloud_backend import app
from nf_cloud_backend.models.workflow import Workflow


class WorkflowsControllers:
    """
    Handles requests regarding the defined nextflow workflows
    """

    @staticmethod
    @app.route("/api/workflows", endpoint="workflow_index")
    def index():
        """
        Returns
        -------
        JSON with key `workflows`, which contains a list of workflow names.
        """
        offset = request.args.get("offset", None)
        limit = request.args.get("limit", None)
        return jsonify({
            "workflows": [
                workflow.to_dict()
                for workflow in Workflow.select().offset(offset).limit(limit)
            ]
        })

    @staticmethod
    @app.route("/api/workflows/published", endpoint="workflow_index_published")
    def index_published():
        """
        Returns
        -------
        JSON with key `workflows`, which contains a list of workflow names.
        """
        offset = request.args.get("offset", None)
        limit = request.args.get("limit", None)
        return jsonify({
            "workflows": {
                workflow.id: workflow.name
                for workflow in Workflow.select().where(Workflow.is_published == True).offset(offset).limit(limit)
            }
        })

    @staticmethod
    @app.route("/api/workflows/create", methods=["POST"], endpoint="workflow_create")
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
        errors: Dict[str, List[Any]] = defaultdict(list)
        error_status_code: int = 422

        data: Dict[str, Any] = request.json
        name: Optional[str] = data.get("name", None)
        description: Optional[str] = data.get("description", None)

        errors = Workflow.validate_name(name, errors)
        errors = Workflow.validate_description(description, errors)

        if len(errors) > 0:
            return jsonify({
                "errors": errors
            }), error_status_code

        workflow: Workflow = Workflow.create(name=name, description=data.get("description", None))
        return jsonify(workflow.to_dict())

    @staticmethod
    @app.route("/api/workflows/<int:workflow_id>", endpoint="workflow_show")
    def show(workflow_id: int):
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
        workflow: Workflow = Workflow.get(Workflow.id == workflow_id)
        workflow_dict = workflow.to_dict()
        if request.args.get("definition_as_text", 0, type=int) > 0:
            workflow_dict["definition"] = json.dumps(workflow.definition, indent=4)
        return jsonify(workflow_dict)

    @staticmethod
    @app.route("/api/workflows/<int:workflow_id>/update", methods=["POST"], endpoint="workflow_update")
    def update(workflow_id: int):
        """
        Endpoint for updating a workflow.
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

        data: Dict[str, Any] = request.json
        definition: Optional[str] = data.get("definition", None)
        description: Optional[str] = data.get("description", None)
        is_published: Optional[bool] = data.get("is_published", None)

        errors = Workflow.validate_description(description, errors)
        errors = Workflow.validate_is_published(is_published, errors)

        workflow: Workflow = Workflow.get(Workflow.id == workflow_id)
        workflow.description = description
        workflow.is_published = is_published

        try:
            workflow.definition = definition
        except json.JSONDecodeError as error:
            errors["definition"].append(f"{error}")

        if workflow.is_published:
            errors = Workflow.validate_definition(workflow.definition, errors)
            if "definition" not in errors:
                workflow.is_validated = True
            else:
                workflow.is_validated = False

        if len(errors) > 0:
            return jsonify({
                "errors": errors
            }), 422

        workflow.save()

        return "", 200

    @staticmethod
    @app.route("/api/workflows/<int:workflow_id>/delete", methods=["POST"], endpoint="workflow_delete")
    def delete(workflow_id: int):
        """
        Endpoint for deleteing a workflow.
        Returns
        -------
        Response
            200 - on success
            422 - on errors
        Raises
        ------
        RuntimeError
            Workflow cannot be delete.
        """

        workflow: Workflow = Workflow.get(Workflow.id == workflow_id)
        workflow.delete_instance()

        return "", 200

    @staticmethod
    @app.route("/api/workflows/<int:workflow_id>/arguments", endpoint="workflow_arguments")
    def arguments(workflow_id: int):
        """
        Endpoint for deleteing a workflow.
        Returns
        -------
        Response
            200 - on success
            422 - on errors
        Raises
        ------
        RuntimeError
            Workflow cannot be delete.
        """

        workflow: Workflow = Workflow.get_or_none(Workflow.id == workflow_id)
        if workflow is None:
            return '', 404
        return jsonify(workflow.definition['args']['dynamic'])
    
    @staticmethod
    @app.route("/api/workflows/<int:workflow_id>/result_definition")
    def result_definition(workflow_id: int):
        workflow: Workflow = Workflow.get_or_none(Workflow.id == workflow_id)
        if workflow is None:
            return '', 404
        return jsonify(workflow.definition['results'])
