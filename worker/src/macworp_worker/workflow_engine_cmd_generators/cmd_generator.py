"""Interface for command generators for workflow runs."""

import logging
from pathlib import Path
from typing import Any, ClassVar, Dict, List

from macworp_utils.exchange.queued_project import QueuedProject
from macworp_utils.path import make_relative_to, secure_joinpath
from macworp_worker.web.backend_web_api_client import BackendWebApiClient


class CmdGenerator:
    """Interface for command generators for workflow runs."""

    WORKFLOW_ENGINE_PARAMETER_PREFIX: ClassVar[str] = ""
    """
    Prefix for workflow engine parameters, e.g. `--` for Nextflow's run command (`-work-dir`) #
    or `--` for Snakemake execution (`--cores`).
    """

    def __init__(
        self,
        workflow_engine_executable: Path,
        backend_web_api_client: BackendWebApiClient,
        logger: logging.Logger,
        weblog_proxy_port: int,
    ):
        self.workflow_engine_executable = workflow_engine_executable
        self.backend_web_api_client = backend_web_api_client
        self.logger = logger
        self.weblog_proxy_port = weblog_proxy_port

    def generate_command(
        self,
        project_dir: Path,
        work_dir: Path,
        project_params: QueuedProject,
        workflow_settings: Dict[str, Any],
    ) -> List[str]:
        """Generate command for running a workflow.

        Parameters
        ----------
        project_dir : Path
            Path to the project directory
        work_dir : Path
            Path to the work directory
        project_params : QueuedProject
            Project parameters
        workflow_settings : Dict[str, Any]
            Workflow definition

        Returns
        -------
        List[str]
            Command to run the workflow using the `subprocess.Popen`.
        """
        raise NotImplementedError("Need to implement this method in a subclass.")

    @classmethod
    def get_workflow_engine_params(cls, workflow_settings: dict) -> List[str]:
        """
        Returns the nextflow run parameters as list
        `[param_name1, param_value1, param_name2, param_value2, ...]`
        as required by the subprocess.Popen() function.

        Parameters
        ----------
        workflow_settings : dict
            Workflow settings

        Returns
        -------
        List[str]
            List of workflow engine parameters ready for command line use
        """
        parameters: List[str] = []
        for param in workflow_settings["engine_parameters"]:
            parameters.append(f"{cls.WORKFLOW_ENGINE_PARAMETER_PREFIX}{param['name']}")
            parameters.append(f"{param['value']}")
        return parameters

    @classmethod
    def process_workflow_param(
        cls,
        project_dir: Path,
        parameter: Dict[str, Any],
        is_static: bool = False,
    ) -> str:
        """
        Process the workflow the given workflow parameter
        and returns the completed value for the command.
        E.g.

        * Argument of type path will be joined to the project directory before converted to string
        * Argument of type paths will be joined to the project directory
            and converted to a comma separated string
        * Argument of type separator will be converted to an empty string
            (best do not pass it to this function at all form the sub class)

        ...


        Parameters
        ----------
        project_params : QueuedProject
            Project parameters
        parameter : Dict[str, Any]
            Workflow parameter for the definition
        is_static : bool
            If the parameter is a static parameter.
            Some parameter options are only available for static parameters.

        Returns
        -------
        str
            Ready to use parameter for the command
        """
        match parameter["type"]:
            case "paths":
                return ",".join(
                    [
                        str(secure_joinpath(project_dir, file))
                        for file in parameter["value"]
                    ]
                )
            case "path":
                path = secure_joinpath(project_dir, parameter["value"])
                if (
                    is_static
                    and "is_relative" in parameter
                    and parameter["is_relative"]
                ):
                    return str(make_relative_to(project_dir, path))
                return str(path)
            case "file-glob":
                return str(secure_joinpath(project_dir, parameter["value"]))
            case "separator":
                return ""
            case _:
                return str(parameter["value"])

    @classmethod
    def cleanup(
        cls,
        project_dir: Path,
        work_dir: Path,
        is_success: bool,
        keep_intermediate_files: bool,
    ) -> None:
        """
        Cleanup after the workflow execution.

        Parameters
        ----------
        project_dir : Path
            Path to the project directory
        work_dir : Path
            Path to the work directory
        is_success : bool
            If the workflow was successful
        keep_intermediate_files : bool
            If the intermediate files should be kept
        """
        raise NotImplementedError("Need to implement this method in a subclass.")
