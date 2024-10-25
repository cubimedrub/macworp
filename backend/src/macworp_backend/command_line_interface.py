# std imports
import argparse

# internal imports
from macworp_backend.models.project import ProjectCommandLineInterface
from macworp_backend.server import Server
from macworp_backend.database import Database
from macworp_backend.utility.command_line_interface import (
    ComandLineInterface as UtilityCLI,
)


class ComandLineInterface:
    def __init__(self):
        self.__parser = argparse.ArgumentParser(description="NF-Cloud backend")
        self.__parser.set_defaults(func=lambda args: self.__parser.print_help())
        subparsers = self.__parser.add_subparsers()
        Server.add_cli_arguments(subparsers)
        Database.add_cli_arguments(subparsers)
        UtilityCLI.add_cli_arguments(subparsers)
        ProjectCommandLineInterface.add_cli_arguments(subparsers)

    def start(self):
        args = self.__parser.parse_args()
        args.func(
            args
        )  # TODO: What happens here: https://docs.python.org/3/library/argparse.html
