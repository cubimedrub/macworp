# std imports
from pathlib import Path

# 3rd party imports
from peewee_migrate import Router
from peewee import PostgresqlDatabase

class Database:
    """
    Class to maintain database.
    """

    @classmethod
    def run_migrations(cls, database_url: str):
        """
        Applies all unapplied migrations.

        Parameters
        ----------
        database_url : str
            PostgreSQL database URL
        """
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
        parser.add_argument("database", type=str, help="Database url")
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
