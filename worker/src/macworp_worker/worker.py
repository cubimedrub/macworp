"""Worker which receives a workflow execution job via a message broker like RabbitMQ."""

# std imports
import functools
import logging
from multiprocessing import Pipe, Queue
from multiprocessing.connection import Connection, wait
from multiprocessing.synchronize import Event as EventClass
from pathlib import Path
from queue import Full as FullQueueError
import time
from threading import Thread
from typing import Any, List, Optional

# external imports
import pika
from pika.channel import Channel

# internal imports
from macworp_worker.logging import get_logger
from macworp_worker.web.backend_web_api_client import BackendWebApiClient
from macworp_worker.web.weblog_proxy import WeblogProxy
from macworp_worker.workflow_executor import WorkflowExecutor


class AckHandler(Thread):
    """
    A separate thread for handling message acknowledgement.
    The communication channel receives tuples with the delivery tag and
    True (for ACK) or False (for NACK).

    Attributes
    ----------
    __broker_connection: pika.BlockingConnection
        Connection to message broker
    __broker_channel: Channel
        Channel to message broker queue
    __comm_channels: List[Connection]
        Connections between this handler and process worker
        for receiving delivery tags for acknowledgement
    """

    def __init__(
        self,
        broker_connection: pika.BlockingConnection,
        broker_channel: Channel,
        comm_channels: List[Connection],
    ):
        super().__init__()
        self.__broker_connection: pika.BlockingConnection = broker_connection
        self.__broker_channel: Channel = broker_channel
        self.__comm_channels: List[Connection] = comm_channels

    def send_ack(self, delivery_tag: Any):
        """
        Sends message acknowledgement to message broker.

        Parameters
        ----------
        delivery_tag : Any
            Message delivery tag.
        """
        if self.__broker_channel.is_open:
            self.__broker_channel.basic_ack(delivery_tag)

    def send_nack(self, delivery_tag: Any):
        """
        Sends message acknowledgement to message broker.

        Parameters
        ----------
        delivery_tag : Any
            Message delivery tag.
        """
        if self.__broker_channel.is_open:
            self.__broker_channel.basic_nack(delivery_tag)

    def run(self):
        """
        Listen on comm channels for incomming delivery tags to acknowledge.
        Stops when all communication channels were closed.
        """
        while len(self.__comm_channels) > 0:
            for comm_channel in wait(self.__comm_channels):
                try:
                    (delivery_tag, is_ack) = comm_channel.recv()

                    # Send the ack threadsafe!
                    callback = (
                        functools.partial(self.send_ack, delivery_tag)
                        if is_ack
                        else functools.partial(self.send_nack, delivery_tag)
                    )
                    self.__broker_connection.add_callback_threadsafe(callback)

                except EOFError:
                    self.__comm_channels.remove(comm_channel)


class Worker:
    """
    Worker which receives a workflow execution job via a message broker like RabbitMQ.
    The workflow is passed to a pool of processes (multiprocessing) for actual execution so the
    main thread is not blocked and is able to send heartbeats to the broker.
    A separate thread (multthreading) handles the message acknowledgement as soon as a process
    sends its delivery tag back over the provided communication channel. This architecture ensures
    a stable connection to the broker and may prevent workflows executions to be rescheduled
    on acknowlegment or heartbeat timeouts.

    Attributes
    ----------
    __nf_bin: Path
        Path to Nextflow binary
    __backend_api_client: NFCloudAPIClient
        Client for communicating with the NFCloud API
    __project_data_path: Path
        Path to folder which contains the separate project folders.
    __rabbit_mq_url: str
        Message broker URL (amqp://...)
    __project_queue_name: str
        Message broker queue
    __connection: Optional[pikaConnection]
        Connection to the message broker
    __channel: Optional[Channel]
        Channel to the message brokers queue
    __number_of_workers: int
        Number of concurrent workers
    __keep_intermediate_files: bool
        Keep work folder after workflow execution
    __stop_event: Event
        Event for stopping worker processes and threads reliable.
    __weblog_proxy: WeblogProxy
        Proxy for sending weblog requests to the NFCloud API using credentials.
    """

    def __init__(
        self,
        nf_bin: Path,
        backend_api_client: BackendWebApiClient,
        projects_data_path: Path,
        rabbit_mq_url: str,
        project_queue_name: str,
        number_of_workers: int,
        keep_intermediate_files: bool,
        stop_event: EventClass,
        log_level: int,
    ):
        # nextflow binary
        self.__nf_bin: Path = nf_bin
        # nextflow cloud attributes
        self.__backend_api_client = backend_api_client
        self.__project_data_path: Path = projects_data_path
        # message broker attributes
        self.__rabbit_mq_url: str = rabbit_mq_url
        self.__project_queue_name: str = project_queue_name
        self.__connection: Optional[pika.BlockingConnection] = None
        self.__channel: Optional[Channel] = None
        # number of workers
        self.__number_of_workers: int = number_of_workers
        # additional worker behavior
        self.__keep_intermediate_files: bool = keep_intermediate_files
        # control
        self.__stop_event: EventClass = stop_event
        self.__log_level: int = log_level
        self.__weblog_proxy = WeblogProxy(backend_api_client, log_level)

    def start(self):
        """
        Starts the worker
        """
        logger = get_logger("worker", self.__log_level)
        logger.info("Starting worker with %i executors.", self.__number_of_workers)

        project_queue = Queue()
        comm_channels: List[Connection] = []  # type: ignore[annotation-unchecked]
        wf_executors: List[WorkflowExecutor] = []  # type: ignore[annotation-unchecked]

        while not self.__stop_event.is_set():
            try:
                self.__connection = pika.BlockingConnection(
                    pika.URLParameters(self.__rabbit_mq_url)
                )
                self.__channel = self.__connection.channel()
                self.__channel.basic_qos(prefetch_count=self.__number_of_workers)

                for _ in range(self.__number_of_workers):
                    ro_comm, rw_comm = Pipe(duplex=False)
                    executor = WorkflowExecutor(
                        self.__nf_bin,
                        self.__backend_api_client,
                        self.__project_data_path,
                        project_queue,
                        rw_comm,
                        self.__keep_intermediate_files,
                        self.__stop_event,
                        self.__log_level,
                        self.__weblog_proxy.port,
                    )
                    executor.start()
                    comm_channels.append(ro_comm)
                    # Close this reference to the read write connection
                    rw_comm.close()
                    wf_executors.append(executor)

                ack_handler = AckHandler(
                    self.__connection, self.__channel, comm_channels
                )
                ack_handler.start()

                logger.info("Executor and AckHandler startet, listening for jobs...")
                # Second return value 'properties' is unnecessary.
                # After 0.5 second it consume will return '(None, None, None)'
                # if no message was send.
                # This will give us time for maintenance, e.g. stop if a stop signal was received
                for method_frame, _, body in self.__channel.consume(
                    self.__project_queue_name, inactivity_timeout=0.5, auto_ack=False
                ):
                    if method_frame and body:
                        try:
                            logger.info("Received job, add it to local queue.")
                            project_queue.put((body, method_frame.delivery_tag))
                        except FullQueueError:
                            pass
                    if self.__stop_event.is_set():
                        break

            except pika.exceptions.ConnectionWrongStateError as error:
                self.__handle_rabbitmq_connection_error(logger, error)
            except pika.exceptions.ConnectionClosed as error:
                self.__handle_rabbitmq_connection_error(logger, error)
            except pika.exceptions.ChannelWrongStateError as error:
                self.__handle_rabbitmq_connection_error(logger, error)
            except pika.exceptions.ChannelClosed as error:
                self.__handle_rabbitmq_connection_error(logger, error)

        # cancel queued messages
        self.__channel.cancel()
        self.__channel.close()
        self.__connection.close()

    def __handle_rabbitmq_connection_error(
        self, logger: logging.Logger, error: BaseException
    ):
        logger.error(
            (
                "RabbitMQ connetion was closed unexpectedly. "
                f"Will try to reconnect in a few seconds. Error was: {error}"
            )
        )
        time.sleep(5)
