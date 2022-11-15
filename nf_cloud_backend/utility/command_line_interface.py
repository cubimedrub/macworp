# std imports
import argparse

# internal imports
from nf_cloud_backend.utility.rabbit_mq import RabbitMQ

class ComandLineInterface:
    @classmethod
    def add_cli_arguments(cls, subparsers: argparse._SubParsersAction):
        """
        Adds cli arguments

        Parameters
        ----------
        subparsers : argparse._SubParsersAction
            Subparser for new arguments
        """
        parser = subparsers.add_parser("utility", help="WebAPI for MaxDecoy")
        parser.set_defaults(func=lambda args: parser.print_help())

        local_subparsers: argparse._SubParsersAction = parser.add_subparsers()
        cls.add_rabbitmq_cli_arguments(local_subparsers)

    @classmethod
    def add_rabbitmq_cli_arguments(cls, subparsers: argparse._SubParsersAction):
        parser = subparsers.add_parser("rabbitmq", help="Helper for managing RabbitMQ")
        parser.set_defaults(func=lambda args: parser.print_help())
        local_subparsers: argparse._SubParsersAction = parser.add_subparsers()
        
        prepare_parser = local_subparsers.add_parser("prepare", help="Creates RabbitMQ queues")
        prepare_parser.set_defaults(func=lambda _cli_args: RabbitMQ.prepare_queues())
        