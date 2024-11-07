"""Constants used in the MAcWorp project."""

# std imports
from enum import Enum, unique


@unique
class SupportedWorkflowEngine(Enum):
    """Supported workflow engines."""

    NEXTFLOW = "nextflow"
    SNAKEMAKE = "snakemake"
    UNSUPPORTED = "unsupported"

    def __str__(self) -> str:
        return self.value
