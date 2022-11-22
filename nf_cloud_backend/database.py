# std imports
from pathlib import Path
from typing import Optional

# 3rd party imports
from peewee_migrate import Router
from peewee import PostgresqlDatabase

# internal imports
from nf_cloud_backend.utility.configuration import Configuration

class Database:
    """
    Class to maintain database.
    """

    @classmethod
    def run_migrations(cls, database_url: Optional[str]):
        """
        Applies all unapplied migrations.

        Parameters
        ----------
        database_url : str
            PostgreSQL database URL
        """
        if database_url is None:
            database_url = Configuration.values()["database"]["url"]
        router = Router(
            PostgresqlDatabase(database_url),
            migrate_dir=str(
                Path(__file__).absolute().parent.joinpath("migrations/")
            )
        )
        router.run()

    @classmethod
    def run_migration_by_cli(cls, cli_args):
        """
        Runs the all migration with arguments provided by CLI

        Parameters
        ----------
        cli_args : Any
            argparse's parsed arguments
        """
        cls.run_migrations(
            cli_args.database
        )


    @classmethod
    def add_migration_subparser(cls, subparsers):
        """
        Adds arguments to the given parser.

        Parameters
        ----------
        subparsers : Any
            argparse's subparsers
        """
        parser = subparsers.add_parser("migrate", help="Runs all unapplied migrations")
        parser.add_argument("--database", "-d", required=False, type=str, help="Overwrites database url from config, optional.")
        parser.set_defaults(func=cls.run_migration_by_cli)


    @classmethod
    def add_cli_arguments(cls, subparsers):
        """
        Adds arguments to the given parser.

        Parameters
        ----------
        subparsers : Any
            argparse's subparsers
        """
        parser = subparsers.add_parser("database", help="Tools for database management.")
        subsubparsers = parser.add_subparsers()
        cls.add_migration_subparser(subsubparsers)
