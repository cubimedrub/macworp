# std imports
from typing import Optional

# 3rd party imports
from flask import Flask

# internal imports
from nf_cloud_backend import app, socketio
from nf_cloud_backend.utility.configuration import Configuration

def get_app() -> Flask:
    """
    Returns the initialized application, e.g. for gunicorn.

    Returns
    -------
    Flask
        Flask application
    """
    return app

class Server:
    """
    Flask web server control.
    """

    @classmethod
    def start(cls, interface: Optional[str] = None, port: Optional[int] = None):
        """
        Starts the flask web server.
        """
        socketio.run(
            app,
            interface if interface is not None else Configuration.values()['interface'],
            port if port is not None else Configuration.values()['port'],
            debug = Configuration.values()['debug'],
            allow_unsafe_werkzeug=Configuration.values()['debug']
        )

    @classmethod
    def start_by_cli(cls, cli_args):
        """
        Starts server with CLI arguments or returns Gunicorn arguments if `--gunicorn` was passed.

        Parameters
        ----------
        cli_args : Any
            Argparse's parsed CLI arguments
        """

        if cli_args.gunicorn is None:
            cls.start(
                cli_args.interface,
                cli_args.port
            )
        else:
            gunicorn_args: str = cli_args.gunicorn
            
            # Add bind from config if not defined in --guncicorn
            if not "-b" in gunicorn_args and not "--bind" in gunicorn_args:
                gunicorn_args += f" -b {Configuration.values()['interface']}:{Configuration.values()['port']} "
            print(
                f"{gunicorn_args} 'nf_cloud_backend.server:get_app()'"
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
        parser.add_argument('--interface', '-i', type=str, required=False, help='Sets on which interface the HQ is running.')
        parser.add_argument('--port', '-p', type=int, required=False, help='Sets on which port the HQ is running.')
        parser.add_argument(
            "--gunicorn",
            nargs='?',
            type=str,
            default=None,                       # When not present
            const="",                           # When present without value
            help=(
                "Can be used as flag or to pass arguments for Gunicorn webserver. "
                "If this is used (even without value), it returns a Gunicorn arguments string "
                "to start MaCPepDB using Gunicorn with the given config file and environment. "
                "Just pass the string to Gunicorn binary. "
                "If no Gunicorn bind option is added (-b|--bind) the interface and port of the config will be used."
            )
        )
        parser.set_defaults(func=cls.start_by_cli)