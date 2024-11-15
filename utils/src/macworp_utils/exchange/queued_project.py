"""Functionality to work with QueuedProject objects."""

# std imports
from typing import Any, Dict, List

# 3rd party imports
from pydantic import BaseModel, Field


class QueuedProject(BaseModel):
    """Reduced representation of the project with all necessary data to run a workflow.

    Parameters
    ----------
    BaseModel : _type_
        _description_
    """

    id: int = 0
    workflow_id: int = 0
    workflow_arguments: List[Dict[str, Any]] = Field(default_factory=list)
