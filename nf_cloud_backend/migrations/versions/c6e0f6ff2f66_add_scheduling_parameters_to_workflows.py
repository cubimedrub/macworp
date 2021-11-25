"""add scheduling parameters to workflows

Revision ID: c6e0f6ff2f66
Revises: 7757066bbc7d
Create Date: 2021-07-16 10:14:16.066552

"""
from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision = 'c6e0f6ff2f66'
down_revision = '7757066bbc7d'
branch_labels = None
depends_on = None


def upgrade():
    op.add_column("workflows", sa.Column("nextflow_workflow", sa.VARCHAR(255), server_default="''"))
    op.add_column("workflows", sa.Column("nextflow_arguments", sa.TEXT, server_default="''"))
    op.add_column("workflows", sa.Column("is_scheduled", sa.BOOLEAN, server_default="false"))


def downgrade():
    op.drop_column("workflows", "nextflow_workflow")
    op.drop_column("workflows", "nextflow_arguments")
    op.drop_column("workflows", "is_scheduled")
