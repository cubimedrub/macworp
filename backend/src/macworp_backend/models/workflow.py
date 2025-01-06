# std imports
import json
from pathlib import Path
from typing import Any, ClassVar, Dict, Union, DefaultDict, List

# 3rd party imports
import jsonschema
from peewee import BigAutoField, CharField, TextField, BooleanField
from playhouse.postgres_ext import JSONField

# internal import
from macworp_backend import db_wrapper as db


class Workflow(db.Model):
    WORKFLOW_SCHEMA_PATH: ClassVar = Path(__file__).parent.parent.joinpath(
        "data/workflow.schema.json"
    )
    """Path to the workflow schema for validation"""

    DEFAULT_DEFINITION_PATH: ClassVar = Path(__file__).parent.parent.joinpath(
        "data/workflow.default.json"
    )
    """Some default workflow definition"""

    id = BigAutoField(primary_key=True)
    name = CharField(max_length=512, null=False)
    description = TextField(null=False)
    _definition = JSONField(
        column_name="definition"
    )  # Has property definition to make sure it is parsed dict before inserting into database
    is_validated = BooleanField(null=False, default=False)
    is_published = BooleanField(null=False, default=False)

    class Meta:
        db_table = "workflows"

    def __init__(self, **kwargs):
        # Make sure definition is parsed dict before inserting into database
        if "definition" in kwargs and isinstance(kwargs["definition"], str):
            kwargs["definition"] = json.loads(kwargs["definition"])
        super().__init__(**kwargs)

    @property
    def definition(self):
        """
        Returns workflow definition

        Returns
        -------
        Dct[str, any]
            Workflow definition
        """
        return self._definition

    @definition.setter
    def definition(self, value: Union[str, Dict[str, any]]):
        """
        Sets workflow definition

        Parameters
        ----------
        value : Union[str, Dict[str, any]]
            Workflow definition
        """
        if isinstance(value, str):
            self._definition = json.loads(value)
        else:
            self._definition = value

    def to_dict(self) -> Dict[str, any]:
        """
        Returns dictionary representations

        Returns
        -------
        Dict[str, any]
            Dictionary representation
        """
        return {
            "id": self.id,
            "name": self.name,
            "definition": self.definition,
            "description": self.description,
            "is_published": self.is_published,
            "is_validated": self.is_validated,
        }

    @classmethod
    def validate_name(
        cls, name: Any, errors: DefaultDict[str, List[str]]
    ) -> DefaultDict[str, List[str]]:
        """
        Validates workflow name

        Parameters
        ----------
        name : str
            Workflow name
        errors : DefaultDict[str, List[str]]
            Dictionary to add errors

        Returns
        -------
        DefaultDict[str, List[str]]
            Dictionaries with errors (errors are stored under the key `name`)
        """
        if not isinstance(name, str):
            errors["name"].append("is not a string")
            return errors
        if name is None:
            errors["name"].append("is missing")
            return errors
        if len(name) < 1:
            errors["name"].append("too short")
        if len(name) > 512:
            errors["name"].append("too long")
        return errors

    @classmethod
    def validate_description(
        cls, description: Any, errors: DefaultDict[str, List[str]]
    ) -> DefaultDict[str, List[str]]:
        """
        Validates workflow description

        Parameters
        ----------
        name : str
            Workflow name
        errors : DefaultDict[str, List[str]]
            Dictionary to add errors

        Returns
        -------
        DefaultDict[str, List[str]]
            Dictionaries with errors (errors are stored under the key `description`)
        """
        if not isinstance(description, str):
            errors["description"].append("is not a string")
            return errors
        if len(description) < 1:
            errors["description"].append("cannot be empty")

        return errors

    @classmethod
    def validate_is_published(
        cls, is_published: Any, errors: DefaultDict[str, List[str]]
    ) -> DefaultDict[str, List[str]]:
        """
        Validates workflow description

        Parameters
        ----------
        name : str
            Workflow name
        errors : DefaultDict[str, List[str]]
            Dictionary to add errors

        Returns
        -------
        DefaultDict[str, List[str]]
            Dictionaries with errors (errors are stored under the key `description`)
        """
        if not isinstance(is_published, bool):
            errors["is_published"].append("is not a boolean")
            return errors
        return errors

    @classmethod
    def validate_definition(
        cls, definition: Any, errors: DefaultDict[str, List[str]]
    ) -> DefaultDict[str, List[str]]:
        """
        Validates workflow description

        Parameters
        ----------
        name : str
            Workflow name
        errors : DefaultDict[str, List[str]]
            Dictionary to add errors

        Returns
        -------
        DefaultDict[str, List[str]]
            Dictionaries with errors (errors are stored under the key `definition`)
        """
        if not isinstance(definition, dict):
            errors["definition"].append("is not a dictionary")
            return errors
        try:
            schema: Dict[Any, Any] = {}
            with cls.WORKFLOW_SCHEMA_PATH.open("r", encoding="utf-8") as schema_file:
                schema = json.loads(schema_file.read())
            jsonschema.validate(instance=definition, schema=schema)
        except jsonschema.exceptions.ValidationError as error:
            errors["definition"].append(error.message)

        return errors
