import json
from pathlib import Path
from typing import TYPE_CHECKING, Dict, Any, ClassVar

import jsonschema
from fastapi import HTTPException
from sqlalchemy import JSON, Column, ForeignKey, Integer
from sqlmodel import Field, Relationship, SQLModel

from macworp.utils.constants import SupportedWorkflowEngine
from .user import User

if TYPE_CHECKING:
    from .project import Project
    from .workflow_share import WorkflowShare


class Workflow(SQLModel, table=True):
    WORKFLOW_SCHEMA_PATH: ClassVar = Path(__file__).parent.parent.joinpath(
        "data/workflow.schema.json"
    )
    DEFAULT_DEFINITION_PATH: ClassVar = Path(__file__).parent.parent.joinpath(
        "data/workflow.default.json"
    )

    id: int | None = Field(default=None, primary_key=True)

    """
    A value of None means this workflow is "orphaned", i. e. the owner got deleted. 
    """
    owner_id: int | None = Field(
        default=None,
        sa_column=Column(
            Integer, ForeignKey("user.id", ondelete="SET NULL"), nullable=True
        ),
    )
    owner: User | None = Relationship(
        back_populates="owned_workflows", sa_relationship_kwargs={"cascade": ""}
    )

    name: str = Field(max_length=512)

    """
    A description in Markdown format.
    """
    description: str = ""

    definition: dict = Field(default_factory=dict, sa_column=Column(JSON))

    """
    Published workflows can be viewed by everyone.
    """
    is_published: bool = False

    shares: list["WorkflowShare"] = Relationship(back_populates="workflow")

    dependent_projects: list["Project"] = Relationship(back_populates="workflow")

    # def validate_workflow_parameters(self, workflow_parameters: List[Dict[str, Any]]):
    #     """
    #     Validate workflow parameters against workflow definition.
    #     """
    #     errors = defaultdict(list)
    #
    #     if "parameters" not in self.definition or "dynamic" not in self.definition["parameters"]:
    #         raise HTTPException(
    #             status_code=422,
    #             detail={"errors": {"general": "workflow definition is invalid"}}
    #         )
    #
    #     # Check if arguments are present
    #     present_params = {
    #         param["name"]
    #         for param in workflow_parameters
    #         if param.get("type") != "separator"
    #     }
    #
    #     for expected_argument in self.definition["parameters"]["dynamic"]:
    #         if expected_argument["type"] == "separator":
    #             continue
    #         if expected_argument["name"] not in present_params:
    #             errors[expected_argument["label"]].append("is missing")
    #
    #     if len(errors) > 0:
    #         raise HTTPException(status_code=422, detail={"errors": dict(errors)})
    #
    #     # Check for empty values
    #     for param in workflow_parameters:
    #         if param.get("type") == "separator":
    #             continue
    #         if "value" not in param or param["value"] is None:
    #             errors[param["label"]].append("cannot be empty")
    #
    #     if len(errors) > 0:
    #         raise HTTPException(status_code=422, detail={"errors": dict(errors)})

    def validate_engine(self, engine: SupportedWorkflowEngine) -> None:
        """
        Workflow type validation
        """
        supported_engines = self.definition.get("supported_engines", [])
        if supported_engines and engine.value not in supported_engines:
            raise HTTPException(
                status_code=400,
                detail=f"This workflow does not support engine '{engine.value}'. Supported: {supported_engines}",
            )

    def get_workflow_params_cache_file(self):
        """
        Get the cache file path for workflow parameters
        """
        cache_dir = Path("cache") / f"project_{self.id}"
        cache_dir.mkdir(parents=True, exist_ok=True)
        return cache_dir / f"workflow_{self.id}_params.json"

    async def validate_workflow_definition(self,
                                           definition: Any
                                           ) -> dict[Any, Any]:
        """
        Validates workflow description
        :param definition: Definition of the workflow
        :return: dict[Any, Any] with errors if any, otherwise an empty dict
        """
        errors = {"definition": []}

        if not isinstance(definition, dict):
            errors["definition"].append("is not a dictionary")
            return errors

        try:
            schema: Dict[Any, Any] = {}
            with self.WORKFLOW_SCHEMA_PATH.open("r", encoding="utf-8") as schema_file:
                schema = json.loads(schema_file.read())
            jsonschema.validate(instance=definition, schema=schema)
        except jsonschema.exceptions.ValidationError as error:
            errors["definition"].append(error.message)

        return errors
