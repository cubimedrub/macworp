# std imports
from pathlib import Path
from typing import Any, ClassVar, Dict, Optional, Set

# 3rd party imports
from peewee_migrate import Router
from peewee import PostgresqlDatabase, IntegrityError
from yaml import load as yaml_load, Loader as YamlLoader

# internal imports
# # Don't remove the model imports, they're needed for seeding
from macworp_backend.models.project import Project  # pylint: disable=unused-import
from macworp_backend.models.user import User  # pylint: disable=unused-import
from macworp_backend.models.workflow import Workflow  # pylint: disable=unused-import
from macworp_backend.utility.configuration import Configuration


class Database:
    """
    Class to maintain database.
    """

    SEED_DATA_PATH: ClassVar[Path] = Path(__file__).parent.parent.parent.joinpath(
        "db_seed.yaml"
    )

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
            migrate_dir=str(Path(__file__).absolute().parent.joinpath("migrations/")),
        )
        router.run()

    @classmethod
    def seed(cls, drop_existing_data: bool):
        """
        Add test data to the database

        Parameters
        ----------
        database_url : str
            PostgreSQL database URL
        """
        # Load seed data
        seeds: Dict[str, Any] = yaml_load(
            cls.SEED_DATA_PATH.read_text(encoding="utf-8"), Loader=YamlLoader
        )["seeds"]
        dropped_models: Set[str] = set()
        for seed in seeds:
            model = eval(seed["model"])  # pylint: disable=eval-used
            # Check if records should be dropped
            if drop_existing_data and seed["model"] not in dropped_models:
                dropped_models.add(seed["model"])
                model.delete().execute()
            # Create record
            try:
                model.create(**seed["attributes"])
            except IntegrityError:
                print(
                    f"[INFO] Model object of type '{seed['model']}' with attributes '{seed['attributes']}' already exists. Skipping ..."
                )

    @classmethod
    def run_migration_by_cli(cls, cli_args):
        """
        Runs the all migration with arguments provided by CLI

        Parameters
        ----------
        cli_args : Any
            argparse's parsed arguments
        """
        cls.run_migrations(cli_args.database)

    @classmethod
    def seed_by_cli(cls, cli_args):
        """
        Runs the all migration with arguments provided by CLI

        Parameters
        ----------
        cli_args : Any
            argparse's parsed arguments
        """
        cls.seed(cli_args.drop)

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
        parser.add_argument(
            "--database",
            "-d",
            required=False,
            type=str,
            help="Overwrites database url from config, optional.",
        )
        parser.set_defaults(func=cls.run_migration_by_cli)

    @classmethod
    def add_seed_subparser(cls, subparsers):
        """
        Adds seed arguments to the given parser.

        Parameters
        ----------
        subparsers : Any
            argparse's subparsers
        """
        parser = subparsers.add_parser(
            "seed", help="Add some test data to the database (only for development)"
        )
        parser.add_argument(
            "--drop",
            required=False,
            default=False,
            action="store_true",
            help="If set, all data will be dropped before seeding.",
        )
        parser.set_defaults(func=cls.seed_by_cli)

    @classmethod
    def add_cli_arguments(cls, subparsers):
        """
        Adds arguments to the given parser.

        Parameters
        ----------
        subparsers : Any
            argparse's subparsers
        """
        parser = subparsers.add_parser(
            "database", help="Tools for database management."
        )
        subsubparsers = parser.add_subparsers()
        cls.add_migration_subparser(subsubparsers)
        cls.add_seed_subparser(subsubparsers)
