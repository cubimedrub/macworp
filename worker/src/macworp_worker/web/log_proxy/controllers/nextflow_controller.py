import logging

from fastapi import Depends, Request

from macworp_utils.constants import SupportedWorkflowEngine  # type: ignore[import]
from macworp_worker.web.log_proxy.settings import Settings, get_dummy_settings


class NextflowController:
    """Controller with all path for proxying Nextflow weblogs to the MAcWorP API."""

    @staticmethod
    async def weblogs(
        project_id: int,
        request: Request,
        settings: Settings = Depends(get_dummy_settings),
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
            settings.client.post_weblog(
                project_id, SupportedWorkflowEngine.NEXTFLOW, log
            )
        # pylint: disable=broad-except
        except Exception as e:
            # Catch everything to prevent the FastAPI server from crashing
            logging.error("Error while sending weblog to MAcWorP API: %s", e)
