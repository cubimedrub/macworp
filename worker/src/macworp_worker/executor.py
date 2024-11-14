"""Executor for running workflows. """

import logging
from multiprocessing import Process, Queue
from multiprocessing.connection import Connection
from multiprocessing.synchronize import Event as EventClass
from pathlib import Path
from queue import Empty as EmptyQueueError
import re
import shutil
import subprocess
from typing import ClassVar, List

from macworp_utils.constants import SupportedWorkflowEngine
from macworp_utils.path import secure_joinpath

from macworp_worker.logging import get_logger
from macworp_worker.web.backend_web_api_client import BackendWebApiClient
from macworp_worker.workflow_engine_cmd_generators.nextflow_cmd_generator import (
    NextflowCmdGenerator,
)
from macworp_worker.workflow_engine_cmd_generators.snakemake_cmd_generator import (
    SnakemakeCmdGenerator,
)


class Executor(Process):
    """
    Executor for running workflows.

    Attributes
    ----------
    nextflow_executable: Path
        Nextflow executable path
    backend_web_api_client: BackendWebApiClient
        Client for communicating with the MAcWorP API
    project_data_path: Path
        Path to folder which contains the separate project folders.
    project_queue: Queue
        Queue for receiving work.
    communication_channel: List[Connection]
        Communication channel with AckHandler for sending delivery tags after work is done.#
    keep_intermediate_files: bool
        Keep work folder after workflow execution.
    stop_event: EventClass
        Event for stopping worker processes and threads reliable.
    log_level: int
        Log level
    weblog_proxy_port: int
        Port for the weblog proxy
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
        nextflow_executable: Path,
        snakemake_executable: Path,
        backend_web_api_client: BackendWebApiClient,
        project_data_path: Path,
        project_queue: Queue,
        communication_channel: List[Connection],
        keep_intermediate_files: bool,
        stop_event: EventClass,
        log_level: int,
        weblog_proxy_port: int,
    ):
        super().__init__()
        self.nextflow_executable: Path = nextflow_executable
        self.snakemake_executable: Path = snakemake_executable
        self.backend_web_api_client = backend_web_api_client
        self.project_data_path: Path = project_data_path
        self.project_queue: Queue = project_queue
        self.communication_channel: List[Connection] = communication_channel
        self.keep_intermediate_files: bool = keep_intermediate_files
        self.stop_event: EventClass = stop_event
        self.log_level: int = log_level
        self.weblog_proxy_port: int = weblog_proxy_port

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
            project_dir = self.project_data_path.joinpath(f"{project_params.id}/")
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
                        self.nextflow_executable,
                        self.backend_web_api_client,
                        logger,
                        self.weblog_proxy_port,
                    ).generate_command(
                        project_dir, work_dir, project_params, workflow["definition"]
                    )
                case SupportedWorkflowEngine.SNAKEMAKE:
                    command = SnakemakeCmdGenerator(
                        self.snakemake_executable,
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
                        self.keep_intermediate_files,
                    )
                case SupportedWorkflowEngine.SNAKEMAKE:
                    SnakemakeCmdGenerator.cleanup(
                        project_dir,
                        work_dir,
                        workflow_process.returncode == 0,
                        self.keep_intermediate_files,
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
                self.communication_channel.send((delivery_tag, False))
                continue

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
