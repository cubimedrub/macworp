"""Peewee migrations -- 003_create users.py.

Some examples (model - class or model name)::

    > Model = migrator.orm['model_name']            # Return model in current state by name

    > migrator.sql(sql)                             # Run custom SQL
    > migrator.python(func, *args, **kwargs)        # Run python code
    > migrator.create_model(Model)                  # Create a model (could be used as decorator)
    > migrator.remove_model(model, cascade=True)    # Remove a model
    > migrator.add_fields(model, **fields)          # Add fields to a model
    > migrator.change_fields(model, **fields)       # Change fields
    > migrator.remove_fields(model, *field_names, cascade=True)
    > migrator.rename_field(model, old_field_name, new_field_name)
    > migrator.rename_table(model, new_table_name)
    > migrator.add_index(model, *col_names, unique=False)
    > migrator.drop_index(model, *col_names)
    > migrator.add_not_null(model, *field_names)
    > migrator.drop_not_null(model, *field_names)
    > migrator.add_default(model, field_name, default)

"""

import datetime as dt
import peewee as pw
from decimal import ROUND_HALF_EVEN

try:
    import playhouse.postgres_ext as pw_pext
except ImportError:
    pass

SQL = pw.SQL


def migrate(migrator, database, fake=False, **kwargs):
    """Write your migrations here."""
    migrator.sql("""
    create table users (
        id bigserial primary key,
        provider_type varchar(256) not null,
        provider varchar(256) not null,
        login_id varchar(512) not null,
        email varchar(512) not null,
        password varchar(4096),
        provider_data json,
        unique (provider, login_id)
    );
    create unique index users_uniqueness_idx on users (provider_type, provider, login_id);
    """)



def rollback(migrator, database, fake=False, **kwargs):
    """Write your rollback migrations here."""
    migrator.sql("""
    drop table users;
    """)

