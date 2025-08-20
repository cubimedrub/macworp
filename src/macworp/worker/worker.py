"""Worker which receives a workflow execution job via a message broker like RabbitMQ."""

import functools
import logging
from multiprocessing import Pipe, Queue
from multiprocessing.connection import Connection, wait
from multiprocessing.synchronize import Event as EventClass
from queue import Full as FullQueueError
import time
from threading import Thread
from typing import Any, List, Optional

import pika
from pika.channel import Channel
from macworp.backend.controllers.depends import Configuration
from macworp.utils.exchange.queued_project import QueuedProject

from macworp.worker.logging import get_logger
from macworp.worker.web.backend_web_api_client import BackendWebApiClient
from macworp.worker.web.log_proxy.server import Server as LogProxy
from macworp.worker.executor import Executor


class AckHandler(Thread):
    """
    A separate thread for handling message acknowledgement.
    The communication channel receives tuples with the delivery tag and
    True (for ACK) or False (for NACK).
    """

    def __init__(
        self,
        broker_connection: pika.BlockingConnection,
        broker_channel: Channel,
        comm_channels: List[Connection],
    ):
        """
        Initializes the AckHandler with the provided broker connection.

        Parameters
        ----------
        broker_connection: pika.BlockingConnection
            Connection to message broker
        broker_channel: Channel
            Channel to message broker queue
        comm_channels: List[Connection]
            Connections between this handler and process worker
            for receiving delivery tags for acknowledgement
        """

        super().__init__()
        self.broker_connection: pika.BlockingConnection = broker_connection
        self.broker_channel: Channel = broker_channel
        self.comm_channels: List[Connection] = comm_channels

    def send_ack(self, delivery_tag: Any):
        """
        Sends message acknowledgement to message broker.

        Parameters
        ----------
        delivery_tag : Any
            Message delivery tag.
        """
        if self.broker_channel.is_open:
            self.broker_channel.basic_ack(delivery_tag)

    def send_nack(self, delivery_tag: Any):
        """
        Sends message acknowledgement to message broker.

        Parameters
        ----------
        delivery_tag : Any
            Message delivery tag.
        """
        if self.broker_channel.is_open:
            self.broker_channel.basic_nack(delivery_tag)

    def run(self):
        """
        Listen on comm channels for incomming delivery tags to acknowledge.
        Stops when all communication channels were closed.
        """
        while len(self.comm_channels) > 0:
            for comm_channel in wait(self.comm_channels):
                try:
                    (delivery_tag, is_ack) = comm_channel.recv()

                    # Send the ack threadsafe!
                    callback = (
                        functools.partial(self.send_ack, delivery_tag)
                        if is_ack
                        else functools.partial(self.send_nack, delivery_tag)
                    )
                    self.broker_connection.add_callback_threadsafe(callback)

                except EOFError:
                    self.comm_channels.remove(comm_channel)


class Worker:
    """
    Worker which receives a workflow execution job via a message broker like RabbitMQ.
    The workflow is passed to a pool of processes (multiprocessing) for actual execution so the
    main thread is not blocked and is able to send heartbeats to the broker.
    A separate thread (multthreading) handles the message acknowledgement as soon as a process
    sends its delivery tag back over the provided communication channel. This architecture ensures
    a stable connection to the broker and may prevent workflows executions to be rescheduled
    on acknowlegment or heartbeat timeouts.
    """

    def __init__(
        self,
        config: Configuration,
        log_level: int,
        stop_event: EventClass,
    ):
        """
        Initializes the worker with the provided configuration and log level.

        Parameters
        ----------
        config : Configuration
            Configuration for the worker.
        log_level : int
            Log level for the worker.
        """
        self.config = config
        self.log_level = log_level
        self.stop_event = stop_event

        self.backend_api_client = BackendWebApiClient(
            config.worker.macworp_url,
            config.worker_credentials.username,
            config.worker_credentials.password,
            not config.worker.skip_cert_verification,
        )

        # message broker attributes
        self.connection: Optional[pika.BlockingConnection] = None
        self.channel: Optional[Channel] = None

        # control
        self.log_proxy = LogProxy(config, log_level)

    def start(self):
        """
        Starts the worker
        """
        logger = get_logger("worker", self.log_level)
        logger.info(
            "Starting worker with %i executors.", self.config.worker.number_of_workers
        )

        project_queue = Queue()
        comm_channels: List[Connection] = []  # type: ignore[annotation-unchecked]
        wf_executors: List[Executor] = []  # type: ignore[annotation-unchecked]

        while not self.stop_event.is_set():
            try:
                self.connection = pika.BlockingConnection(
                    pika.URLParameters(self.config.rabbitmq.url)
                )
                self.channel = self.connection.channel()
                self.channel.basic_qos(
                    prefetch_count=self.config.worker.number_of_workers
                )

                for _ in range(self.config.worker.number_of_workers):
                    ro_comm, rw_comm = Pipe(duplex=False)
                    executor = Executor(
                        self.config,
                        self.log_level,
                        self.stop_event,
                        self.backend_api_client,
                        self.log_proxy.port,
                        rw_comm,
                        project_queue,
                    )
                    executor.start()
                    comm_channels.append(ro_comm)
                    # Close this reference to the read write connection
                    rw_comm.close()
                    wf_executors.append(executor)

                ack_handler = AckHandler(self.connection, self.channel, comm_channels)
                ack_handler.start()

                logger.info("Executor and AckHandler startet, listening for jobs...")
                # Second return value 'properties' is unnecessary.
                # After 0.5 second it consume will return '(None, None, None)'
                # if no message was send.
                # This will give us time for maintenance, e.g. stop if a stop signal was received
                for method_frame, _, body in self.channel.consume(
                    self.config.rabbitmq.project_workflow_queue,
                    inactivity_timeout=0.5,
                    auto_ack=False,
                ):
                    if method_frame and body:
                        try:
                            logger.info("Received job, add it to local queue.")
                            project_params = QueuedProject.model_validate_json(body)
                            project_queue.put(
                                (project_params, method_frame.delivery_tag)
                            )
                        except FullQueueError:
                            pass
                    if self.stop_event.is_set():
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
        self.channel.cancel()
        self.channel.close()
        self.connection.close()

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
