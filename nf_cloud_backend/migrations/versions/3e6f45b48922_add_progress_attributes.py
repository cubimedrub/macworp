"""add progress attributes

Revision ID: 3e6f45b48922
Revises: ec3fb66dccd4
Create Date: 2021-12-09 13:58:50.803799

"""
from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision = '3e6f45b48922'
down_revision = 'ec3fb66dccd4'
branch_labels = None
depends_on = None


def upgrade():
    op.drop_column("workflows", "nextflow_log")
    op.add_column("workflows", sa.Column("submitted_processes", sa.Integer, server_default="0"))
    op.add_column("workflows", sa.Column("completed_processes", sa.Integer, server_default="0"))


def downgrade():
    op.drop_column("workflows", "submitted_processes")
    op.drop_column("workflows", "completed_processes")
    op.add_column("workflows", sa.Column("nextflow_log", sa.dialects.postgresql.JSONB, server_default="{}"))
