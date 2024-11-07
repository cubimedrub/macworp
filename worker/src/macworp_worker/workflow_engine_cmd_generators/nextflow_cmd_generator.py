"""Generates the command for executing a Nextflow workflow."""

from pathlib import Path
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
                f"http://127.0.0.1:{self.weblog_proxy_port}/projects"
                f"/{project_params.id}/{SupportedWorkflowEngine.NEXTFLOW}"
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

        # Add workflow static parameters
        for argument in workflow_settings["parameters"]["static"]:
            command.append(f"--{argument['name']}")
            command.append(argument["value"])

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
            case _:
                raise ValueError(
                    f"Unsupported workflow location: {workflow_source['type']}"
                )

    def get_workflow_arguments(
        self,
        project_dir: Path,
        workflow_arguments: List[Dict[str, Any]],
    ) -> List[str]:
        """
        Processes the workflow arguments

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
                self.process_workflow_param(project_dir, argument)
            )

        return processed_arguments
