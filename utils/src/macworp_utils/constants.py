"""Constants used in the MAcWorp project."""

# std imports
from enum import Enum, unique
from typing import Self


@unique
class SupportedWorkflowEngine(Enum):
    """Supported workflow engines."""

    NEXTFLOW = "nextflow"
    SNAKEMAKE = "snakemake"
    UNSUPPORTED = "unsupported"

    def __str__(self) -> str:
        return self.value

    @classmethod
    def from_str(cls, value: str) -> Self:
        """
        Converts a string to a SupportedWorkflowEngine.

        Parameters
        ----------
        value : str
            String value

        Returns
        -------
        SupportedWorkflowEngine
            Supported workflow engine

        Raises
        ------
        ValueError
            If the string value is not supported

        """
        for engine in cls:
            if engine.value == value:
                return engine
        raise ValueError(f"Unsupported workflow engine: {value}")


WEBLOG_WORKFLOW_ENGINE_HEADER = "X-Workflow-Engine-Type"
