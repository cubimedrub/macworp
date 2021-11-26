"""add log to workflow

Revision ID: b12d9af1e45d
Revises: c6e0f6ff2f66
Create Date: 2021-11-25 07:22:26.749743

"""
from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision = 'b12d9af1e45d'
down_revision = 'c6e0f6ff2f66'
branch_labels = None
depends_on = None


def upgrade():
    op.add_column("workflows", sa.Column("nextflow_log", sa.dialects.postgresql.JSONB, server_default="{}"))


def downgrade():
    op.drop_column("workflows", "nextflow_log")
