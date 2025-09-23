"""Controller for Snakemake logs"""

import logging
from typing import Annotated

from fastapi import Query, Request

from macworp.utils.constants import SupportedWorkflowEngine  # type: ignore[import]
from macworp.worker.web.log_proxy.controllers.depends import BackendWebApiClient  # type: ignore[import]


class SnakemakeController:
    """
    Controller has the mimics the same API as Panoptes as this is what Snakemake expects
    when using `--wms-monitor`.
    See https://snakemake.readthedocs.io/en/stable/executing/monitoring.html for more info.
    """

    @staticmethod
    async def service_info():
        """Gives information about the service status"""
        return {
            "status": "running",
        }

    @staticmethod
    async def create_workflow(project_id: Annotated[int | None, Query(gt=0)]):
        """
        Loops back the project_id

        Parameters
        ----------
        project_id : int
            Project ID as query parameter
        """
        return {"id": project_id}

    @staticmethod
    async def update_workflow_status(request: Request, client: BackendWebApiClient):
        """
        Converts log from form data to JSON and forwards it to the MAcWorP API.
        """
        form = await request.form()
        if "id" not in form or "msg" not in form:
            return
        project_id = form.get("id")
        log = form.get("msg")
        try:
            if not isinstance(project_id, str):
                raise ValueError("Project ID is not a string")
            if not isinstance(log, str):
                raise ValueError("Log is not a string")
            client.post_weblog(
                int(project_id), SupportedWorkflowEngine.SNAKEMAKE, log.encode("utf-8")
            )
        # pylint: disable=broad-except
        except Exception as e:
            # Catch everything to prevent the FastAPI server from crashing
            logging.error("Error while sending weblog to MAcWorP API: %s", e)

    @staticmethod
    async def workflow(project_id: int):
        pass
