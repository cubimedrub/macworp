# std imports
from pathlib import Path
from multiprocessing import Event
import signal

# 3rd party imports
import yaml
from mergedeep import merge

# internal imports
from nf_cloud_worker.comand_line_interface import ComandLineInterface as CLI
from nf_cloud_worker.web.nf_cloud_web_api_client import NFCloudWebApiClient
from nf_cloud_worker.worker import Worker


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

    cli: CLI = CLI()

    worker = Worker(
        Path(cli.arguments.nf_bin).absolute(),
        NFCloudWebApiClient(
            cli.arguments.nf_cloud_url,
            cli.arguments.api_user,
            cli.arguments.api_password
        ),
        Path(cli.arguments.projects_data_path).absolute(),
        cli.arguments.rabbitmq_url,
        cli.arguments.project_queue_name,
        cli.arguments.number_of_workers,
        stop_event,
        cli.arguments.verbose
    )
    worker.start()

if __name__ == "__main__":
    main()
