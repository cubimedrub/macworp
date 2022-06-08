# std imports
import pathlib
import re
import subprocess
from dataclasses import dataclass
from typing import ClassVar, List

# external imports
from mergedeep import merge

@dataclass
class Workflow:
    """
    Running workflows
    """

    PRECEDING_SLASH_REGES: ClassVar[re.Pattern] = re.compile(r"^/+")

    __slots__ = [
        "__nf_bin",
        "__work_dir",
        "__workflow_path",
        "__nextflow_main_script_name",
        "__fix_nextflow_paramters",
        "__dynamic_nextflow_arguments",
        "__static_workflow_arguments",
        "__nextflow_weblog_url",
    ]

    __nf_bin: pathlib.Path
    __work_dir: pathlib.Path
    __workflow_path: pathlib.Path
    __nextflow_main_script_name: str
    __fix_nextflow_paramters: list
    __dynamic_nextflow_arguments: dict
    __static_workflow_arguments: dict
    __nextflow_weblog_url: str

    def __init__(self, nf_bin: pathlib.Path, work_dir: pathlib.Path, workflow_path: pathlib.Path, nextflow_main_script_name: list, fix_nextflow_paramters: str, dynamic_nextflow_arguments: dict, static_workflow_arguments: dict, nextflow_weblog_url: str):
        self.__nf_bin = nf_bin
        self.__work_dir = work_dir
        self.__workflow_path = workflow_path
        self.__nextflow_main_script_name = nextflow_main_script_name
        self.__fix_nextflow_paramters = fix_nextflow_paramters
        self.__dynamic_nextflow_arguments = dynamic_nextflow_arguments
        self.__static_workflow_arguments = static_workflow_arguments
        self.__nextflow_weblog_url = nextflow_weblog_url

    def _pre_workflow_arguments(self) -> List[str]:
        """
        Arguments which are add before the nextflow command. Useful for `firejail`.
        
        Returns
        -------
        List of arguments, added before the nextflow command.
        """
        return []

    def _nextflow_run_parameters(self) -> List[str]:
        """
        Arguments for `nextflow run` (without script itself), e.g. `-work-dir` or `-with-weblog`.
        See:

        `-work-dir` and `-with-weblog` are coverd by AbstractWorkflow._nextflow_run_parameters()

        Returns
        -------
        List of `nextflow run` parameters 
        """
        return [
            "-work-dir",
            str(self.__work_dir),
            "-with-weblog",
            self.__nextflow_weblog_url
        ] + self.__fix_nextflow_paramters

    def _post_workflow_arguments(self) -> List[str]:
        """
        AbstractWorkflow._post_workflow_arguments() returns the given nextflow arguments

        Returns
        -------
        Arguments after nextflow command, e.g. workflow paramters: `--in-file foo.txt --out-file bar.txt`
        """
        return []

    def __nextflow_main_scrip_path(self) -> pathlib.Path:
        """
        Returns path the nextflow main script.

        Returns
        -------
        Path to main script
        """
        return self.__workflow_path.joinpath(self.__nextflow_main_script_name)

    def _nextflow_command(self) -> List[str]:
        """
        Nextflow command, inclusive nextflow run parameters and script.

        Returns
        -------
        Nextflow command as list
        """
        return self._pre_workflow_arguments() \
            + [
                str(self.__nf_bin),
                "run"
            ] \
            + self._nextflow_run_parameters() \
            + self.__get_arguments_as_list() \
            + [str(self.__nextflow_main_scrip_path())] \
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
        String without preceding whitespaces

        """
        return cls.PRECEDING_SLASH_REGES.sub("", some_string)

    def __get_arguments_as_list(self) -> list:
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
            self.__dynamic_nextflow_arguments if self.__dynamic_nextflow_arguments is not None else {},
            self.__static_workflow_arguments if self.__static_workflow_arguments is not None else {}
        )
        for arg_name, arg_definition in merged_arguments.items():
            arg_value = arg_definition["value"]
            # Special argument type handling
            if arg_definition["type"] == "number":
                arg_value = str(arg_value)
            elif arg_definition["type"] == "paths":
                arg_value = ",".join([
                    str(self.__work_dir.joinpath(
                        self.__class__.remove_preceding_slash(file))
                    ) for file in arg_value
                ])
            elif arg_definition["type"] == "path":
                arg_value = str(self.__work_dir.joinpath(
                    self.__class__.remove_preceding_slash(arg_value)
                ))
            elif arg_definition["type"] == "file-glob":
                # Path.joinpath removes wildcards. So we need to append 
                # the value manually to the path.
                arg_value = f"{self.__work_dir}/{self.__class__.remove_preceding_slash(arg_value)}"
            workflow_arguments.append(f"--{arg_name}")
            workflow_arguments.append(arg_value)
            
        return workflow_arguments

    def start(self):
        """
        Runs the workflow
        """
        print("nextflow arguments:", self._nextflow_command())
        nf_proc = subprocess.Popen(self._nextflow_command(), cwd=self.__work_dir, text = True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        ls_stdout, ls_stderr = nf_proc.communicate()
        print("stdout:\n", ls_stdout)
        print("stderr:\n", ls_stderr)
        

