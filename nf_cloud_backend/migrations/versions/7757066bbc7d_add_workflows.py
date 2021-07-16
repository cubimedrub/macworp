"""add workflows

Revision ID: 7757066bbc7d
Revises: 
Create Date: 2021-07-14 13:38:35.563221

"""
from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision = '7757066bbc7d'
down_revision = None
branch_labels = None
depends_on = None


def upgrade():
    op.create_table(
        "workflows",
        sa.Column("id", sa.BigInteger, primary_key=True),
        sa.Column("name", sa.VARCHAR(512), nullable=False),
    )


def downgrade():
    op.drop_table("workflows")
