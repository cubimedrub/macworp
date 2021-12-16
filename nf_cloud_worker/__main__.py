# std imports
import pathlib
import signal
from threading import Event

# external imports
import yaml
from mergedeep import merge

# internal imports
from nf_cloud_worker.comand_line_interface import ComandLineInterface as CLI
from nf_cloud_worker.worker import Worker


def main():
    stop_event = Event()

    def handle_stop_signals(signum, frame):
        print("receive stop signal. stopping worker ...")
        stop_event.set()

    signal.signal(signal.SIGTERM, handle_stop_signals)
    signal.signal(signal.SIGINT, handle_stop_signals)

    cli = CLI()

    workflows = {}
    for workflow_config_path_option in cli.arguments.workflows:
        workflow_config_path = pathlib.Path(workflow_config_path_option)
        if workflow_config_path.is_file():
            with workflow_config_path.open("r") as workflow_file:
                merge(
                    workflows,
                    yaml.load(workflow_file, Loader=yaml.CLoader)["workflows"]
                )

    worker = Worker(
        pathlib.Path(cli.arguments.nf_bin),
        cli.arguments.nf_cloud_url,
        cli.arguments.rabbitmq_url,
        cli.arguments.workflow_queue,
        pathlib.Path(cli.arguments.workflow_data_path).absolute(),
        workflows,
        stop_event
    )
    worker.start()

if __name__ == "__main__":
    main()
