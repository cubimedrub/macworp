"""Command line interface for access backend, frontend and tools"""

import argparse
from logging import CRITICAL, INFO, NOTSET, WARNING, ERROR, DEBUG
from pathlib import Path
from typing import ClassVar

from macworp.configuration import Configuration
from macworp.backend.database import (
    init_database_schema,
    is_database_connectable,
    seed_database,
)
from macworp.backend.app import start_app as start_backend_app
from macworp.frontend.app import start_app as start_frontend_app
from macworp.utils.rabbitmq import RabbitMQ
from macworp.worker.app import start_app as start_worker_app
from macworp.utils.authentication.worker_credentials import hash_worker_credentials


class ConfigurationLoadAction(argparse.Action):
    """
    If the `--config` argument is given,
    this action loads the configuration from the specified file.
    """

    def __call__(self, parser, namespace, values, option_string=None):
        config = Configuration.from_file(values)
        setattr(namespace, self.dest, config)


class CommandLineInterface:
    """Command line interface for MacWorP."""

    DB_SEED_COMMAND: ClassVar[str] = "db:seed"
    """Command to seed the database."""

    DB_INIT_COMMAND: ClassVar[str] = "db:init"
    """Command to migrate the database"""

    DB_IS_CONNECTABLE_COMMAND: ClassVar[str] = "db:is-connectable"
    """Command to check if the database is connectable."""

    CONFIG_PRINT_COMMAND: ClassVar[str] = "config:print"
    """Command to print the current configuration."""

    BACKEND_START_COMMAND: ClassVar[str] = "backend:start"
    """Command to start the backend server."""

    FRONTEND_START_COMMAND: ClassVar[str] = "frontend:start"
    """Command to start the frontend server."""

    HASH_WORKER_CREDENTIALS_COMMAND: ClassVar[str] = "worker:hash-credentials"
    """Command to hash worker credentials."""

    WORKER_START_COMMAND: ClassVar[str] = "worker:start"
    """Command to start the worker."""

    RABBITMQ_INIT_COMMAND: ClassVar[str] = "rabbitmq:init"
    """Command to initialize the RabbitMQ queues."""

    RABBITMQ_IS_CONNECTABLE_COMMAND: ClassVar[str] = "rabbitmq:is-connectable"
    """Command to check if RabbitMQ is connectable."""

    def __init__(self):
        self.parser = argparse.ArgumentParser(description="MacWorP")
        subparsers = self.parser.add_subparsers(dest="command")
        self.add_common_arguments()
        self.add_utility_arguments(subparsers)
        self.add_backend_arguments(subparsers)
        self.add_frontend_arguments(subparsers)
        self.add_worker_arguments(subparsers)

        self.args = self.parser.parse_args()

        # Set default configuration if not provided
        if self.args.config is None:
            setattr(self.args, "config", Configuration())

    def add_common_arguments(self):
        """Add common arguments like the config file"""
        self.parser.add_argument(
            "--config",
            type=Path,
            help="Path to the configuration file",
            action=ConfigurationLoadAction,
        )
        self.parser.set_defaults(func=self.parser.print_help)

        self.parser.add_argument(
            "--verbose",
            "-v",
            default=0,
            action="count",
            help="Increase verbosity of the output. Can be used multiple times.",
        )

    def add_utility_arguments(self, subparsers: argparse._SubParsersAction):
        """Adds utility arguments, e.g. printing of default config, database initialization, ...

        Parameters
        ----------
        subparsers : argparse._SubParsersAction
            Subparsers to add new parser
        """
        database_seed_parser = subparsers.add_parser(
            self.DB_SEED_COMMAND, help="Seed the database"
        )
        database_seed_parser.add_argument(
            "--seed-path",
            type=Path,
            help="Path to the seed file",
            default="./db_seed.yml",
        )
        database_seed_parser.set_defaults(
            func=lambda args: seed_database(args.config, args.seed_path)
        )

        database_init_parser = subparsers.add_parser(
            self.DB_INIT_COMMAND,
            help="Initializes the database",
        )
        database_init_parser.set_defaults(
            func=lambda args: init_database_schema(args.config)
        )

        database_is_connectable_parser = subparsers.add_parser(
            self.DB_IS_CONNECTABLE_COMMAND,
            help="Checks if the database is connectable, exits with 0 if yes, 1 if no",
        )
        database_is_connectable_parser.set_defaults(
            func=lambda args: (
                exit(0) if is_database_connectable(args.config) else exit(1)
            )
        )

        # pylint: disable=unreachable
        print_config_parser = subparsers.add_parser(
            self.CONFIG_PRINT_COMMAND, help="Print the default configuration"
        )
        print_config_parser.set_defaults(func=lambda args: print(str(Configuration())))

        parse_config_parser = subparsers.add_parser(
            "config:parse",
            help="Replaced the `ENV['<ENV_VAR>']` with the given related environment variables",
        )
        parse_config_parser.add_argument(
            "config_file",
            type=Path,
            help="Path to the configuration file",
        )
        parse_config_parser.set_defaults(
            func=lambda args: print(str(Configuration.from_file(args.config_file)))
        )

        rabbitmq_init_parser = subparsers.add_parser(
            self.RABBITMQ_INIT_COMMAND, help="Initializes the RabbitMQ queues"
        )
        rabbitmq_init_parser.set_defaults(
            func=lambda args: RabbitMQ.init_queues(
                args.config.rabbitmq.url,
                args.config.rabbitmq.project_workflow_queue,
            )
        )

        rabbitmq_is_connectable_parser = subparsers.add_parser(
            self.RABBITMQ_IS_CONNECTABLE_COMMAND,
            help="Checks if RabbitMQ is connectable, exits with 0 if yes, 1 if no",
        )
        rabbitmq_is_connectable_parser.set_defaults(
            func=lambda args: (
                exit(0)
                if RabbitMQ.is_connectable(args.config.rabbitmq.url)
                else exit(1)
            )
        )

    def add_backend_arguments(self, subparsers: argparse._SubParsersAction):
        """Adds backend controls

        Parameters
        ----------
        subparsers : argparse._SubParsersAction
            Subparsers to add new parser
        """
        start_backend_parser = subparsers.add_parser(
            self.BACKEND_START_COMMAND, help="Starts the backend server"
        )
        start_backend_parser.set_defaults(
            func=lambda args: start_backend_app(args.config)
        )

    def add_frontend_arguments(self, subparsers: argparse._SubParsersAction):
        """Adds backend controls

        Parameters
        ----------
        subparsers : argparse._SubParsersAction
            Subparsers to add new parser
        """
        start_frontend_parser = subparsers.add_parser(
            self.FRONTEND_START_COMMAND, help="Starts the frontend"
        )
        start_frontend_parser.set_defaults(
            func=lambda args: start_frontend_app(args.config)
        )

    def add_worker_arguments(self, subparsers: argparse._SubParsersAction):
        """Adds worker controls

        Parameters
        ----------
        subparsers : argparse._SubParsersAction
            Subparsers to add new parser
        """
        start_frontend_parser = subparsers.add_parser(
            self.WORKER_START_COMMAND, help="Starts the worker"
        )
        start_frontend_parser.set_defaults(
            func=lambda args: start_worker_app(
                args.config, self.verbosity_to_log_level(args.verbose)
            )
        )

        hash_worker_credentials_parser = subparsers.add_parser(
            self.HASH_WORKER_CREDENTIALS_COMMAND,
            help="Prints the hashed worker credentials to use with e.g. cURL",
        )
        hash_worker_credentials_parser.set_defaults(
            func=lambda args: print(
                hash_worker_credentials(
                    args.config.worker_credentials.username,
                    args.config.worker_credentials.password,
                )
            )
        )

    @classmethod
    def verbosity_to_log_level(cls, verbosity: int) -> int:
        """Returns the log level based on the verbosity argument.

        Returns
        -------
        int
            Log level corresponding to the verbosity argument.
        """
        match verbosity:
            case 0:
                return NOTSET
            case 1:
                return CRITICAL
            case 2:
                return ERROR
            case 4:
                return WARNING
            case 3:
                return INFO
            case _:
                return DEBUG
