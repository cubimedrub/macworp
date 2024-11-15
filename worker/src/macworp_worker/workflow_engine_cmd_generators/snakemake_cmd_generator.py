"""Generates the command for executing a Nextflow workflow."""

from pathlib import Path
import shutil
from typing import Any, ClassVar, Dict, List

from git import Repo as GitRepo

from macworp_utils.exchange.queued_project import QueuedProject  # type: ignore[import-untyped]
from macworp_utils.constants import SupportedWorkflowEngine  # type: ignore[import-untyped]
from macworp_worker.workflow_engine_cmd_generators.cmd_generator import CmdGenerator


class SnakemakeCmdGenerator(CmdGenerator):
    """Executes a workflow on a project."""

    WORKFLOW_ENGINE_PARAMETER_PREFIX: ClassVar[str] = "--"
    """Prefix for workflow engine parameters"""

    LOG_DIR_NAME_IN_CACHE_DIR: ClassVar[str] = "log"
    """Name of the log directory in the cache directory"""

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
            "--directory",
            str(project_dir),
            "--default-resources",
            f"tmpdir='{str(work_dir)}'",
            "--wms-monitor",
            (
                f"http://127.0.0.1:{self.weblog_proxy_port}/{SupportedWorkflowEngine.SNAKEMAKE}"
            ),
            "--wms-monitor-arg",
            f"project_id={project_params.id}",
        ]

        # Add developer defined workflow engine parameters, e.g. "-profile docker"
        command += self.__class__.get_workflow_engine_params(workflow_settings)

        # Add workflow source
        command += self.get_workflow_source(workflow_settings, work_dir=work_dir)

        config_params = []

        # Add workflow dynamic parameters
        config_params += self.get_workflow_arguments(
            project_dir, project_params.workflow_arguments
        )

        # Add workflow dynamic parameters
        config_params += self.get_workflow_arguments(
            project_dir, workflow_settings["parameters"]["static"], is_static=True
        )

        if len(config_params) > 0:
            command.append("--config")
            command.append(" ".join(config_params))

        return command

    def get_workflow_source(
        self, workflow_settings: Dict[str, Any], work_dir: Path
    ) -> List[str]:
        """
        Returns the snakefile option.
        If the workflow source is remote, the repository is cloned to the work directory.
        """

        workflow_source = workflow_settings["src"]
        match workflow_source["type"]:
            case "local":
                directory = Path(workflow_source["directory"]).absolute()
                directory = directory.joinpath(workflow_source["script"])
                return ["--snakefile", str(directory)]
            case "remote":
                local_repo_path = work_dir.joinpath("workflow_repo")
                if not local_repo_path.exists():
                    GitRepo.clone_from(
                        workflow_source["url"],
                        local_repo_path,
                        multi_options=[f"--branch {workflow_source['version']}"],
                    )
                else:
                    repo = GitRepo(local_repo_path)
                    repo.remotes.origin.fetch()
                    repo.git.checkout(workflow_source["version"])
                return [
                    "--snakefile",
                    str(local_repo_path.joinpath("Snakefile")),
                ]
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

        return [
            f"{argument['name']}='{self.process_workflow_param(project_dir, argument, is_static)}'"
            for argument in workflow_arguments
            if argument["type"] != "separator"
        ]

    @classmethod
    def cleanup(
        cls,
        project_dir: Path,
        work_dir: Path,
        is_success: bool,
        keep_intermediate_files: bool,
    ) -> None:
        snakemake_cache_dir = project_dir.joinpath(".snakemake")
        if not keep_intermediate_files:
            if work_dir.is_dir():
                shutil.rmtree(work_dir, ignore_errors=True)
            if snakemake_cache_dir.is_dir():
                if is_success:
                    # delete the complete cache directory on success
                    shutil.rmtree(snakemake_cache_dir, ignore_errors=True)
                else:
                    # delete everything except the log directory on failure
                    for node in snakemake_cache_dir.iterdir():
                        if node.is_dir() and node.name != cls.LOG_DIR_NAME_IN_CACHE_DIR:
                            shutil.rmtree(node, ignore_errors=True)
