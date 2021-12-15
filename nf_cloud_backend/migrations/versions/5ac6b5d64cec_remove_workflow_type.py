"""remove workflow type

Revision ID: 5ac6b5d64cec
Revises: 3e6f45b48922
Create Date: 2021-12-15 10:53:27.423252

"""
from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision = '5ac6b5d64cec'
down_revision = '3e6f45b48922'
branch_labels = None
depends_on = None


def upgrade():
    op.drop_column("workflows", "nextflow_workflow_type")


def downgrade():
    op.add_column("workflows", sa.Column("nextflow_workflow_type", sa.VARCHAR(255), server_default="''"))
