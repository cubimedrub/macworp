"""add type to workflows

Revision ID: 35dd10eb2ecd
Revises: b12d9af1e45d
Create Date: 2021-11-29 12:53:39.297622

"""
from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision = '35dd10eb2ecd'
down_revision = 'b12d9af1e45d'
branch_labels = None
depends_on = None



def upgrade():
    op.add_column("workflows", sa.Column("nextflow_workflow_type", sa.VARCHAR(255), server_default="''"))


def downgrade():
    op.drop_column("workflows", "nextflow_workflow_type")