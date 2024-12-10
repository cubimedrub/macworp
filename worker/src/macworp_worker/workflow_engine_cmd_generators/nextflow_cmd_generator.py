"""Generates the command for executing a Nextflow workflow."""

from pathlib import Path
import shutil
from typing import Any, ClassVar, Dict, List

from macworp_utils.exchange.queued_project import QueuedProject  # type: ignore[import-untyped]
from macworp_utils.constants import SupportedWorkflowEngine  # type: ignore[import-untyped]
from macworp_worker.workflow_engine_cmd_generators.cmd_generator import CmdGenerator


class NextflowCmdGenerator(CmdGenerator):
    """Executes a workflow on a project."""

    WORKFLOW_ENGINE_PARAMETER_PREFIX: ClassVar[str] = "-"
    """Prefix for workflow engine parameters"""

    def generate_command(
        self,
        project_dir: Path,
        work_dir: Path,
        project_params: QueuedProject,
        workflow_settings: Dict[str, Any],
    ) -> List[str]:
        # Start `nextflow run -work-dir ... -with-weblog ...`
        command = [
            str(self.workflow_engine_executable),
            "run",
            "-work-dir",
            str(work_dir),
            "-with-weblog",
            (
                f"http://127.0.0.1:{self.weblog_proxy_port}/{SupportedWorkflowEngine.NEXTFLOW.value}"
                f"/projects/{project_params.id}"
            ),
        ]

        # Add developer defined workflow engine parameters, e.g. "-profile docker"
        command += self.__class__.get_workflow_engine_params(workflow_settings)

        # Add workflow source
        command += self.get_workflow_source(workflow_settings)

        # Add workflow dynamic parameters
        command += self.get_workflow_arguments(
            project_dir, project_params.workflow_arguments
        )

        # Add workflow dynamic parameters
        command += self.get_workflow_arguments(
            project_dir, workflow_settings["parameters"]["static"], is_static=True
        )

        return command

    def get_workflow_source(self, workflow_settings: Dict[str, Any]) -> List[str]:
        """Returns the workflow source as list of strings."""

        workflow_source = workflow_settings["src"]
        match workflow_source["type"]:
            case "local":
                directory = Path(workflow_source["directory"]).absolute()
                directory = directory.joinpath(workflow_source["script"])
                return [str(directory)]
            case "remote":
                source = [workflow_source["url"]]
                if "version" in workflow_source:
                    source.append("-r")
                    source.append(workflow_source["version"])
                return source
            case "nf-core":
                return [f"nf-core/{workflow_source['pipeline']}"]
            case _:
                raise ValueError(
                    f"Unsupported workflow location: {workflow_source['type']}"
                )

    def get_workflow_arguments(
        self,
        project_dir: Path,
        workflow_arguments: List[Dict[str, Any]],
        is_static: bool = False,
    ) -> List[str]:
        """
        Processes the workflow arguments

        Parameters
        ----------
        project_dir : Path
            Path to the project directory
        workflow_arguments : List[Dict[str, Any]]
            List of workflow arguments
        is_static : bool
            If the parameters are static parameters.
            Some parameter options are only available for static parameters.

        Returns
        -------
        List[str]
            List of processed arguments ready for command line
        """
        processed_arguments = []
        for argument in workflow_arguments:
            if argument["type"] == "separator":
                continue
            processed_arguments.append(f"--{argument['name']}")
            processed_arguments.append(
                self.process_workflow_param(project_dir, argument, is_static)
            )

        return processed_arguments

    @classmethod
    def cleanup(
        cls,
        project_dir: Path,
        work_dir: Path,
        is_success: bool,
        keep_intermediate_files: bool,
    ) -> None:
        if not keep_intermediate_files:
            nextflow_cache_dir = project_dir.joinpath(".nextflow")
            if work_dir.is_dir():
                shutil.rmtree(work_dir, ignore_errors=True)
            if nextflow_cache_dir.is_dir():
                shutil.rmtree(nextflow_cache_dir, ignore_errors=True)
        if is_success:
            project_dir.joinpath(".nextflow.log").unlink()
