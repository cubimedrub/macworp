# std imports
import json
from typing import Any, Dict, Union

# 3rd party imports
from peewee import (
    BigAutoField,
    CharField,
    TextField,
    BooleanField
)
from playhouse.postgres_ext import JSONField

# internal import
from nf_cloud_backend import db_wrapper as db

class Workflow(db.Model):
    id = BigAutoField(primary_key=True)
    name = CharField(max_length=512, null=False)
    description = TextField(null=False)
    _definition = JSONField(column_name="definition")   # Has property definition to make sure it is parsed dict before inserting into database
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
            "is_validated": self.is_validated
        }