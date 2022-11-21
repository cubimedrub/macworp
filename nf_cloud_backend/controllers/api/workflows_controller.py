# external imports
from flask import jsonify

# internal imports
from nf_cloud_backend import app
from nf_cloud_backend.utility.configuration import Configuration


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
                for workflow in Configuration.values()["workflows"].keys()
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
            "arguments": Configuration.values()["workflows"][workflow]["args"]["dynamic"]
        })