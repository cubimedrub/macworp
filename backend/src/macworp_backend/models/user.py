# std imports
from typing import Any, Dict

# 3rd party imports
from flask_login import UserMixin
from peewee import BigAutoField, CharField, SQL
from playhouse.postgres_ext import JSONField

# internal imports
from macworp_backend import db_wrapper


class User(db_wrapper.Model, UserMixin):
    """
    User implementation for login and permission management.
    """

    id = BigAutoField(primary_key=True)
    provider_type = CharField(max_length=256, null=False)
    provider = CharField(max_length=256, null=False)
    login_id = CharField(max_length=512, null=False)
    email = CharField(max_length=512, null=False)
    password = CharField(
        max_length=512, null=True
    )  # may be null, when the provider is not local
    provider_data = JSONField()  # JSON field to store provider data, e.g. auth tokens

    class Meta:
        """
        Additional ORM information.
        """

        db_table = "users"
        indexes = (
            # Unique index for provider and login_id
            (("provider_type", "provider", "login_id"), True)
        )
        constraints = [SQL("unique(provider_type, provider, login_id)")]

    def to_dict(self) -> Dict[str, Any]:
        """
        Returns user as dictionary without password.

        Returns
        -------
        Dict[str, Any]
            User dictionary
        """
        return {
            "id": self.id,
            "provider_type": self.provider_type,
            "provider": self.provider,
            "login_id": self.login_id,
            "email": self.email,
        }
