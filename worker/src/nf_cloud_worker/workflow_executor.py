# std imports
import json
import logging
from multiprocessing import Process, Event, Queue
from multiprocessing.connection import Connection
from pathlib import Path
from queue import Empty as EmptyQueueError
import re
import subprocess
from typing import Any, ClassVar, Dict, List

# 3rd party imports
from mergedeep import merge

# internal imports
from nf_cloud_worker.logging import get_logger
from nf_cloud_worker.web.nf_cloud_web_api_client import NFCloudWebApiClient

class WorkflowExecutor(Process):
    """
    Executes a workflow on a project.

    Attributes
    ----------
    __nf_bin: Path
        Path to Nextflow binary
    __nf_cloud_web_api_client: NFCloudWebApiClient
        Client for communicating with the NFCloud API
    __project_data_path: Path
        Path to folder which contains the separate project folders.
    __project_queue: Queue
        Queue for receiving work.
    __communication_channel: List[Connection]
        Communication channel with AckHandler for sending delivery tags after work is done.
    __stop_event: Event
        Event for stopping worker processes and threads reliable.
    __log_level: int
        Log level
    __weblog_proxy_port: int
        Port for the weblog proxy
    """

    PRECEDING_SLASH_REGEX: ClassVar[re.Pattern] = re.compile(r"^/+")
    """Regex for finding preceeding slashes.
    """

    SANITZE_REGEX: ClassVar[re.Pattern] = re.compile(r"[^\w\d\ ]+")
    WHITESPACE_REGEX: ClassVar[re.Pattern] = re.compile(r"\s+")

    def __init__(self, nf_bin: Path, nf_cloud_web_api_client: NFCloudWebApiClient, project_data_path: Path, project_queue: Queue,
        communication_channel: Connection, stop_event: Event, log_level: int, weblog_proxy_port: int):
        super().__init__()
        # Nextflow binary
        self.__nf_bin: Path = nf_bin
        # NF-cloud attributes
        self.__nf_cloud_web_api_client = nf_cloud_web_api_client
        self.__project_data_path: Path = project_data_path
        # Project queue
        self.__project_queue: Queue = project_queue
        # Communication channel with AckHandler
        self.__communication_channel: List[Connection] = communication_channel
        # Event for breaking work loop
        self.__stop_event: Event = stop_event
        self.__log_level = log_level
        self.__weblog_proxy_port = weblog_proxy_port

    def _pre_workflow_arguments(self) -> List[str]:
        """
        Arguments which are added before the nextflow command. Useful for running nextflow in `firejail`.
        
        Returns
        -------
        List of arguments, added before the nextflow command.
        """
        return []

    def _nextflow_run_parameters(self, work_dir: Path, nextflow_weblog_url: str, fix_nextflow_paramters: List[Any]) -> List[str]:
        """
        Arguments for `nextflow run` (without script itself), e.g. `-work-dir` or `-with-weblog`.
        See: https://www.nextflow.io/docs/latest/cli.html#run
        `-work-dir` and `-with-weblog` are coverd already added here.

        Returns
        -------
        List of `nextflow run` parameters
        """
        return [
            "-work-dir",
            str(work_dir),
            "-with-weblog",
            nextflow_weblog_url
        ] + fix_nextflow_paramters

    def _post_workflow_arguments(self) -> List[str]:
        """
        WorkflowExecutor._post_workflow_arguments() returns the given nextflow arguments

        Returns
        -------
        Arguments after nextflow command, e.g. workflow paramters: `--in-file foo.txt --out-file bar.txt`
        """
        return []

    def _nextflow_command(self, project_dir: Path, work_dir: Path, nextflow_weblog_url: str, fix_nextflow_paramters: List[str],
        dynamic_nextflow_arguments: dict, static_workflow_arguments: Dict[str, Any], nextflow_main_scrip_path: Path) -> List[str]:
        """
        Nextflow command, inclusive nextflow run parameters and script.

        Parameters
        ----------
        project_dir : Path
            Path to projects data directory
        work_dir : Path
            Path to 
        nextflow_weblog_url : str
            _description_
        fix_nextflow_paramters : List[str]
            _description_
        dynamic_nextflow_arguments : dict
            _description_
        static_workflow_arguments : dict
            _description_
        nextflow_main_scrip_path : Path
            _description_

        Returns
        -------
        List[str]
            `nextflow run` parameter list for using with subprocess
        """
        return self._pre_workflow_arguments() \
            + [
                str(self.__nf_bin),
                "run"
            ] \
            + self._nextflow_run_parameters(
                work_dir,
                nextflow_weblog_url,
                fix_nextflow_paramters
            ) \
            + [str(nextflow_main_scrip_path)] \
            + self.__get_arguments_as_list(
                project_dir,
                dynamic_nextflow_arguments,
                static_workflow_arguments
            ) \
            + self._post_workflow_arguments()

    @classmethod
    def remove_preceding_slash(cls, some_string: str) -> str:
        """
        Removes preceding slashes from given sting.
        Useful to ensure new path segments which are joined with the workdir
        stay within the workdir.
        If a path segment with preceding slash is merged with another path,
        the new path will become an absolute path.

        Parameters
        ----------
        some_string : str

        Returns
        -------
        String without preceding slash

        """
        return cls.PRECEDING_SLASH_REGEX.sub("", some_string)

    def __get_arguments_as_list(self, project_dir: Path, dynamic_nextflow_arguments: dict, static_workflow_arguments: dict) -> list:
        """
        Merges the static and dynamic arguments and creates
        a list of argument names and values to pass to the subprocess.

        Returns
        -------
        Nextflow process argument as list.
        """
        workflow_arguments = []
        # Merge arguments
        merged_arguments = merge(
            {},
            dynamic_nextflow_arguments if dynamic_nextflow_arguments is not None else {},
            static_workflow_arguments if static_workflow_arguments is not None else {}
        )
        for arg_name, arg_definition in merged_arguments.items():
            arg_value = arg_definition["value"]
            # Convert argument if necessary
            if "type" in arg_definition:
                # Special argument type handling
                if arg_definition["type"] == "number":
                    arg_value = str(arg_value)
                elif arg_definition["type"] == "paths":
                    arg_value = ",".join([
                        str(project_dir.joinpath(
                            self.__class__.remove_preceding_slash(file))
                        ) for file in arg_value
                    ])
                elif arg_definition["type"] == "path":
                    arg_value = str(project_dir.joinpath(
                        self.__class__.remove_preceding_slash(arg_value)
                    ))
                elif arg_definition["type"] == "file-glob":
                    # Path.joinpath removes wildcards. So we need to append
                    # the value manually to the path.
                    arg_value = f"{project_dir}/{self.__class__.remove_preceding_slash(arg_value)}"
            else:
                # Convert everything else to string for subprocess.Popen
                arg_value = str(arg_value)
            workflow_arguments.append(f"--{arg_name}")
            workflow_arguments.append(arg_value)

        return workflow_arguments

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
        sanitized_name: str = self.__class__.SANITZE_REGEX.sub("", name)
        return self.__class__.WHITESPACE_REGEX.sub("_", sanitized_name)

    def run(self):
        """
        Processes work from the queue until the stop_event is set.
        """
        logger = get_logger("executor", self.__log_level)
        logger.info("Starting workflow executor.")

        while not self.__stop_event.is_set():
            try:
                (body, delivery_tag) = self.__project_queue.get(timeout=5)
            except EmptyQueueError:
                continue

            # Parse project parameters from message broker
            project_params: dict = json.loads(body)
            logger.info(f"Processing project: {project_params['id']}")
            # Project work dir
            project_dir: Path = self.__project_data_path.joinpath(f"{project_params['id']}/")
            # Get workflow settings
            workflow: Dict[str, Any] = self.__nf_cloud_web_api_client.get_workflow(project_params["workflow_id"])

            nextflow_main_scrip_path = self.__get_workflow_main_script_path(
                workflow["definition"]
            )

            fix_nextflow_paramters = self.__get_fix_nextflow_parameters(
                workflow["definition"]
            )

            dynamic_nextflow_arguments = {
                argument["name"]: argument
                for argument in project_params["workflow_arguments"]
            }

            static_workflow_arguments = self.__get_workflow_static_arguments(
                workflow["definition"]
            )

            # Create temporary work dir for Nextflow intermediate files.
            work_dir: Path = project_dir.joinpath(
                self.sanitize_workflow_name(workflow["name"])
            )
            if not work_dir.is_dir():
                work_dir.mkdir(parents=True, exist_ok=True)

            nextflow_command: List[str] = self._nextflow_command(
                project_dir,
                work_dir,
                f"http://127.0.0.1:{self.__weblog_proxy_port}/projects/{project_params['id']}",
                fix_nextflow_paramters,
                dynamic_nextflow_arguments,
                static_workflow_arguments,
                nextflow_main_scrip_path
            )

            logger.info(f"Project {project_params['id']}: `{' '.join(nextflow_command)}`")

            nf_proc = subprocess.Popen(
                nextflow_command,
                cwd=project_dir,
                text = True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE
            )
            _nf_stdout, _nf_stderr = nf_proc.communicate()

            # Send delivery tag to thread for acknowledgement
            self.__communication_channel.send(delivery_tag)

            # Report project as finished
            self.__nf_cloud_web_api_client.post_finish(project_params["id"])

            logger.info(f"Finished project: {project_params['id']}")

    def __get_workflow_main_script_path(self, workflow_settings: dict) -> Path:
        return Path(workflow_settings["directory"]) \
            .absolute() \
            .joinpath(workflow_settings["script"])

    def __get_workflow_static_arguments(self, workflow_settings: dict) -> Dict[str, Any]:
        """
        Returns the static arguments as dict `{param_name1: param1, param_name2: param2, ...}`
        So it can be merged with the dynamic arguments.

        Parameters
        ----------
        workflow_settings : Dict[str, Any]
            Workflow settings

        Returns
        -------
        Dict[str, Any]
            Static arguments
        """
        return {
            param["name"]: param
            for param in workflow_settings["args"]["static"]
        }

    def __get_fix_nextflow_parameters(self, workflow_settings: dict) -> List[str]:
        """
        Returns the nextflow run parameters  as list `[param_name1, param_value1, param_name2, param_value2, ...]`
        as required by the subprocess.Popen() function.

        Parameters
        ----------
        workflow_settings : dict
            Workflow settings
        
        Returns
        -------
        List[str]
        """
        parameters: List[str] = []
        for param in workflow_settings["nextflow_parameters"]:
            parameters.append(f"-{param['name']}")
            parameters.append(f"{param['value']}")
        return parameters