from pydantic import BaseModel
from typing import List, Dict, Any


class QueuedProject(BaseModel):
    """
    Model representing a project queued for execution in RabbitMQ.
    """
    id: int  # Project ID
    workflow_id: int  # Workflow ID
    workflow_arguments: List[Dict[str, Any]] # Workflow Params
