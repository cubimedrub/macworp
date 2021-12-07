"""change nextflow_arguments to JSONB

Revision ID: ec3fb66dccd4
Revises: 35dd10eb2ecd
Create Date: 2021-11-30 09:50:50.191723

"""
from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision = 'ec3fb66dccd4'
down_revision = '35dd10eb2ecd'
branch_labels = None
depends_on = None


def upgrade():
    op.drop_column("workflows", "nextflow_arguments")
    op.add_column("workflows", sa.Column("nextflow_arguments", sa.dialects.postgresql.JSONB, server_default="{}"))


def downgrade():
    op.drop_column("workflows", "nextflow_arguments")
    op.add_column("workflows", sa.Column("nextflow_arguments", sa.TEXT, server_default="''"))
