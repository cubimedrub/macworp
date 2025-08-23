"""Entry point for the worker application."""

from multiprocessing import Event
import signal

from macworp.configuration import Configuration
from macworp.worker.worker import Worker


def start_app(config: Configuration, log_level: int) -> None:
    """
    Main function which parses CLI-commands and starts worker.
    """
    stop_event = Event()

    def handle_stop_signals(_signum, _frame):
        print("receive stop signal. stopping worker ...")
        stop_event.set()

    signal.signal(signal.SIGTERM, handle_stop_signals)
    signal.signal(signal.SIGINT, handle_stop_signals)

    worker = Worker(config, log_level, stop_event)
    worker.start()
