# std imports
import argparse
from dataclasses import dataclass
from typing import Any

@dataclass
class ComandLineInterface:

    __slots__ = [
        "__arg_parser",
        "__arguments"
    ]

    __arg_parser: argparse.ArgumentParser
    __arguments: argparse.Namespace

    def __init__(self):
        self.__arg_parser = argparse.ArgumentParser(description="NF Cloud worker")
        self.__arg_parser.add_argument("--nf-cloud-url", "-c", type=str, help="Cloud URL")
        self.__arg_parser.add_argument("--rabbitmq-url", "-r", type=str, help="RabbitMQ URL")
        self.__arg_parser.add_argument("--workflow-queue", "-q", type=str, help="Name of the workflow queue")
        self.__arg_parser.add_argument("--workflow-data-path", "-d", type=str, help="Root folder of the workflow data folders.")
        self.__arg_parser.add_argument("--workflows", "-w", action="append", type=str, help="YAML-file(s) which contains the the workflow definitions, like the on for NF-Cloud.")

        self.__arguments = self.__arg_parser.parse_args()

    @property
    def arguments(self) -> argparse.Namespace:
        return self.__arguments

