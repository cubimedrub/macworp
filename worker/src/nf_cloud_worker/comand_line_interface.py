# std imports
import argparse
from dataclasses import dataclass
from typing import Any


@dataclass
class ComandLineInterface:

    __slots__ = ["__arg_parser", "__arguments"]

    __arg_parser: argparse.ArgumentParser
    __arguments: argparse.Namespace

    def __init__(self):
        self.__arg_parser = argparse.ArgumentParser(description="NF Cloud worker")
        self.__arg_parser.add_argument(
            "--nf-bin", "-n", type=str, help="Path to nextflow binary."
        )
        self.__arg_parser.add_argument(
            "--nf-cloud-url", "-c", type=str, help="Cloud URL"
        )
        self.__arg_parser.add_argument(
            "--rabbitmq-url", "-r", type=str, help="RabbitMQ URL"
        )
        self.__arg_parser.add_argument(
            "--project-queue-name", "-q", type=str, help="Name of the workflow queue"
        )
        self.__arg_parser.add_argument(
            "--projects-data-path",
            "-d",
            type=str,
            help="Root folder of the workflow data folders.",
        )
        self.__arg_parser.add_argument(
            "--number-of-workers",
            "-t",
            type=int,
            default=1,
            required=False,
            help="Number of concurrent workers.",
        )
        self.__arg_parser.add_argument("--api-user", "-u", type=str, help="API user.")
        self.__arg_parser.add_argument(
            "--api-password", "-p", type=str, help="API password."
        )
        self.__arg_parser.add_argument(
            "--verbose", "-v", default=0, action="count", help="Verbose"
        )
        self.__arg_parser.add_argument(
            "--keep-intermediate-files",
            default=False,
            action="store_true",
            help="Keep work folder, workflow engine logs and cache folder after workflow execution. (default: False)",
        )

        self.__arguments = self.__arg_parser.parse_args()

    @property
    def arguments(self) -> argparse.Namespace:
        return self.__arguments
