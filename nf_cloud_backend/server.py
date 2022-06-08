# std imports
from typing import Optional

# internal imports
from nf_cloud_backend import app, env, config, socketio
from nf_cloud_backend.utility.configuration import Environment

class Server:
    """
    Flask web server control.
    """

    @classmethod
    def start(cls, interface: Optional[str] = None, port: Optional[int] = None, environment: Optional[Environment] = None):
        """
        Starts the flask web server.
        """
        global env
        if environment is not None:
            env = environment
        print(f"Start NF-Cloud webinterface in {env.name} mode on {interface}:{config['port']}")
        socketio.run(
            app,
            interface if interface is not None else config['interface'],
            port if port is not None else config['port']
        )

    @classmethod
    def start_by_cli(cls, cli_args):
        """
        Starts web server with the given CLI arguments

        Parameters
        ----------
        cli_args : Any
            Argparse's parsed CLI arguments
        """
        cls.start(
            cli_args.interface,
            cli_args.port,
            cli_args.environment
        )


    @classmethod
    def add_cli_arguments(cls, subparsers):
        """
        Adds arguments to the given parser.

        Parameters
        ----------
        subparsers : Any
            Argparse subparser
        """
        parser = subparsers.add_parser("serve", help="Starts webserver")
        parser.add_argument('--environment', '-e', required=False, choices=[Environment.production.name, Environment.development.name], help='Sets the execution environment (Overrides environment variable MDCHQ_ENV).')
        parser.add_argument('--interface', '-i', type=str, required=False, help='Sets on which interface the HQ is running.')
        parser.add_argument('--port', '-p', type=int, required=False, help='Sets on which port the HQ is running.')
        parser.set_defaults(func=cls.start_by_cli)