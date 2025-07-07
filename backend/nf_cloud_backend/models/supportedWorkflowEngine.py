from enum import Enum

from fastapi import HTTPException


class SupportedWorkflowEngine(str, Enum):
    NEXTFLOW = "nextflow"
    SNAKEMAKE = "snakemake"
    CWL = "cwl"

    @classmethod
    def from_str(cls, value: str) -> "SupportedWorkflowEngine":
        try:
            return cls(value.lower())
        except ValueError:
            raise HTTPException(
                status_code=400,
                detail=f"Unsupported workflow engine: '{value}'. Supported engines: {[e.value for e in cls]}"
            )
