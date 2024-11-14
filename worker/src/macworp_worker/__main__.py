"""Entry point for the worker application."""

# std imports
from pathlib import Path
from multiprocessing import Event
import signal

# internal imports
from macworp_worker.comand_line_interface import ComandLineInterface as CLI
from macworp_worker.logging import verbosity_to_log_level
from macworp_worker.web.backend_web_api_client import BackendWebApiClient
from macworp_worker.worker import Worker


def main():
    """
    Main function which parses CLI-commands and starts worker.
    """
    stop_event = Event()

    def handle_stop_signals(_signum, _frame):
        print("receive stop signal. stopping worker ...")
        stop_event.set()

    signal.signal(signal.SIGTERM, handle_stop_signals)
    signal.signal(signal.SIGINT, handle_stop_signals)

    cli = CLI()

    log_level = verbosity_to_log_level(cli.arguments.verbose)

    worker = Worker(
        Path(cli.arguments.nf_bin).absolute(),
        Path(cli.arguments.sm_bin).absolute(),
        BackendWebApiClient(
            cli.arguments.macworp_url,
            cli.arguments.api_user,
            cli.arguments.api_password,
            not cli.arguments.skip_cert_verification,
        ),
        Path(cli.arguments.projects_data_path).absolute(),
        cli.arguments.rabbitmq_url,
        cli.arguments.project_queue_name,
        cli.arguments.number_of_workers,
        cli.arguments.keep_intermediate_files,
        stop_event,
        log_level,
    )
    worker.start()


if __name__ == "__main__":
    main()
