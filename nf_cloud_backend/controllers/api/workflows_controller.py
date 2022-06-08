# external imports
from flask import jsonify

# internal imports
from nf_cloud_backend import app, config


class WorkflowControllers:
    """
    Handles requests regarding the defined nextflow workflows
    """

    @staticmethod
    @app.route("/api/workflows")
    def workflows():
        """
        Returns
        -------
        JSON with key `nextflow_workflows`, which contains a list of workflow names.
        """
        return jsonify({
            "workflows": sorted([
                workflow
                for workflow in config["workflows"].keys()
            ])
        })

    @staticmethod
    @app.route("/api/workflows/<string:workflow>/arguments")
    def arguments(workflow: str):
        """
        Returns
        -------
        JSON where each key is the name of a workflow argument with value type definition and description.
        """
        return jsonify({
            "arguments": config["workflows"][workflow]["args"]["dynamic"]
        })