"""Executor for running workflows."""

import logging
from multiprocessing import Process, Queue
from multiprocessing.connection import Connection
from multiprocessing.synchronize import Event as EventClass
from pathlib import Path
from queue import Empty as EmptyQueueError
import re
import subprocess
from typing import ClassVar, List

from macworp.configuration import Configuration
from macworp.utils.constants import SupportedWorkflowEngine
from macworp.utils.path import secure_joinpath

from macworp.worker.logging import get_logger
from macworp.worker.web.backend_web_api_client import BackendWebApiClient
from macworp.worker.workflow_engine_cmd_generators.nextflow_cmd_generator import (
    NextflowCmdGenerator,
)
from macworp.worker.workflow_engine_cmd_generators.snakemake_cmd_generator import (
    SnakemakeCmdGenerator,
)


class Executor(Process):
    """
    Executor for running workflows.
    """

    PRECEDING_SLASH_REGEX: ClassVar[re.Pattern] = re.compile(r"^/+")
    """Regex for finding preceeding slashes.
    """

    SANITIZE_REGEX: ClassVar[re.Pattern] = re.compile(r"[^\w\d\ ]+")
    """Regex matching all characters that are not alphanumeric or whitespace."""

    WHITESPACE_REGEX: ClassVar[re.Pattern] = re.compile(r"\s+")
    """Regex matching whitespaces."""

    def __init__(
        self,
        config: Configuration,
        log_level: int,
        stop_event: EventClass,
        backend_web_api_client: BackendWebApiClient,
        weblog_proxy_port: int,
        communication_channel: List[Connection],
        project_queue: Queue,
    ):
        super().__init__()
        self.config = config
        self.log_level = log_level
        self.stop_event = stop_event
        self.backend_web_api_client = backend_web_api_client
        self.weblog_proxy_port = weblog_proxy_port
        self.communication_channel = communication_channel
        self.project_queue = project_queue

    def run(self):
        """
        Processes work from the queue until the stop_event is set.
        """
        logger = get_logger("executor", self.log_level)
        logger.info("Starting workflow executor.")

        while not self.stop_event.is_set():
            try:
                (project_params, delivery_tag) = self.project_queue.get(timeout=5)
            except EmptyQueueError:
                continue

            logger.info("[WORKER / PROJECT %i] Start", project_params.id)

            try:
                if self.backend_web_api_client.is_project_ignored(project_params.id):
                    logger.warning(
                        (
                            "[WORKER / PROJECT %i] Project is ignored. "
                            "Removing from queue and moving on.",
                        ),
                        project_params.id,
                    )
                    self.communication_channel.send((delivery_tag, True))
                    continue
            except Exception as e:  # pylint: disable=broad-except
                logging.error(
                    (
                        "[WORKER / PROJECT %i] Not able to fetch project ignore status from API. "
                        "Rejecting message and moving on: %s"
                    ),
                    project_params.id,
                    e,
                )
                self.communication_channel.send((delivery_tag, False))
                continue

            # Project work dir
            project_dir = self.config.projects_path.absolute().joinpath(
                f"{project_params.id}/"
            )
            # Get workflow settings
            workflow = {}

            try:
                workflow = self.backend_web_api_client.get_workflow(
                    project_params.workflow_id
                )
            except Exception as e:  # pylint: disable=broad-except
                logging.error(
                    (
                        "[WORKER / PROJECT %i] Not able to fetch workflow from API. "
                        "Rejecting message and moving on: %s",
                    ),
                    project_params.id,
                    e,
                )
                self.communication_channel.send((delivery_tag, False))
                continue

            # Create a temporary work directory for the workflow
            sanitized_workflow_name = self.sanitize_workflow_name(workflow["name"])
            work_dir = secure_joinpath(
                project_dir, Path(f".{sanitized_workflow_name}_work")
            )
            if not work_dir.is_dir():
                work_dir.mkdir(parents=True, exist_ok=True)

            command = []

            try:
                workflow_engine = SupportedWorkflowEngine.from_str(
                    workflow["definition"]["engine"]
                )
            except ValueError as e:
                logger.error(
                    "[WORKER / PROJECT %i] Unsupported workflow engine: %s",
                    project_params.id,
                    workflow["definition"]["engine"],
                )
                self.communication_channel.send((delivery_tag, False))
                continue

            match workflow_engine:
                case SupportedWorkflowEngine.NEXTFLOW:
                    command = NextflowCmdGenerator(
                        self.config.worker.nextflow_binary,
                        self.backend_web_api_client,
                        logger,
                        self.weblog_proxy_port,
                    ).generate_command(
                        project_dir, work_dir, project_params, workflow["definition"]
                    )
                case SupportedWorkflowEngine.SNAKEMAKE:
                    command = SnakemakeCmdGenerator(
                        self.config.worker.snakemake_binary,
                        self.backend_web_api_client,
                        logger,
                        self.weblog_proxy_port,
                    ).generate_command(
                        project_dir, work_dir, project_params, workflow["definition"]
                    )

            logger.debug(
                "[WORKER / PROJECT %i] %s",
                project_params.id,
                " ".join(command),
            )

            workflow_process = subprocess.Popen(
                command,
                cwd=project_dir,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            workflow_stdout, workflow_stderr = workflow_process.communicate()

            match workflow_engine:
                case SupportedWorkflowEngine.NEXTFLOW:
                    NextflowCmdGenerator.cleanup(
                        project_dir,
                        work_dir,
                        workflow_process.returncode == 0,
                        self.config.worker.keep_intermediate_files,
                    )
                case SupportedWorkflowEngine.SNAKEMAKE:
                    SnakemakeCmdGenerator.cleanup(
                        project_dir,
                        work_dir,
                        workflow_process.returncode == 0,
                        self.config.worker.keep_intermediate_files,
                    )

            if workflow_process.returncode != 0:
                logger.error(
                    (
                        "[WORKER / PROJECT %i] Workflow execution failed:"
                        "\n----stdout----\n%s"
                        "\n----stderr----\n%s"
                    ),
                    project_params.id,
                    workflow_stdout.replace("\n", "\n\t"),
                    workflow_stderr.replace("\n", "\n\t"),
                )

            # Send delivery tag to thread for acknowledgement
            self.communication_channel.send((delivery_tag, True))

            logger.debug("send delivery tag")

            try:
                self.backend_web_api_client.post_finish(project_params.id)
                logger.debug("finished")
            except ConnectionError as e:
                logging.error(
                    (
                        "[WORKER / PROJECT %i] Could not mark the project as finished. "
                        "Please do that manually an check why the web API is not available: %s"
                    ),
                    project_params.id,
                    e,
                )

            logger.info("[WORKER / PROJECT %i] finished", project_params.id)

    def sanitize_workflow_name(self, name: str) -> str:
        """
        Removes special characters from given name

        Parameters
        ----------
        name : str
            Some string

        Returns
        -------
        str
            Sanitized string
        """
        sanitized_name: str = self.__class__.SANITIZE_REGEX.sub("", name)
        return self.__class__.WHITESPACE_REGEX.sub("_", sanitized_name)
