import logging

from fastapi import Request

from macworp.utils.constants import SupportedWorkflowEngine
from macworp.worker.web.log_proxy.controllers.depends import BackendWebApiClient  # type: ignore[import]


class NextflowController:
    """Controller with all path for proxying Nextflow weblogs to the MAcWorP API."""

    @staticmethod
    async def weblogs(
        project_id: int,
        request: Request,
        client: BackendWebApiClient,
    ):
        """
        Receives the weblogs from Nextflow and forwards them to the MAcWorP API.

        Parameters
        ----------
        project_id : ints
            Project ID
        """
        log = await request.body()
        try:
            client.post_weblog(project_id, SupportedWorkflowEngine.NEXTFLOW, log)
        # pylint: disable=broad-except
        except Exception as e:
            # Catch everything to prevent the FastAPI server from crashing
            logging.error("Error while sending weblog to MAcWorP API: %s", e)
