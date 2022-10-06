# std imports
import functools
from multiprocessing import Event, Pipe, Queue
from multiprocessing.connection import Connection, wait
from pathlib import Path
from queue import Full as FullQueueError
import time
from threading import Thread
from typing import Any, List, Optional

# external imports
import pika
from pika.channel import Channel

# internal imports
from nf_cloud_worker.workflow_executor import WorkflowExecutor

class AckHandler(Thread):
    """
    A separate thread for handling message acknowledgement as soon a delivery tag
    is send over the given communication channels.

    Attributes
    ----------
    __broker_connection: pika.BlockingConnection
        Connection to message broker
    __broker_channel: Channel
        Channel to message broker queue
    __comm_channels: List[Connection]
        Connections between this handler and process worker for receiving delivery tags for acknowledgement
    """

    def __init__(self, broker_connection: pika.BlockingConnection, broker_channel: Channel, comm_channels: List[Connection]):
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

    def run(self):
        """
        Listen on comm channels for incomming delivery tags to acknowledge.
        Stops when all communication channels were closed.
        """
        while len(self.__comm_channels) > 0:
            for comm_channel in wait(self.__comm_channels):
                try:
                    delivery_tag: Any = comm_channel.recv()

                    # Send the ack threadsafe!
                    ack_callback = functools.partial(self.send_ack, delivery_tag)
                    self.__broker_connection.add_callback_threadsafe(ack_callback)

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
    __nf_cloud_url: str
        Base URL for NF-Cloud
    __nf_cloud_api_user: str
        NF-Cloud API user name
    __nf_cloud_api_password: str
        NF-Cloud API user password
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
    __workflows: dict
        Defined workflows
    __number_of_workers: int
        Number of concurrent workers
    __stop_event: Event
        Event for stopping worker processes and threads reliable.
    """

    def __init__(self, nf_bin: Path, nf_cloud_url: str, nf_cloud_api_user: str,
        nf_cloud_api_password: str, projects_data_path: Path, rabbit_mq_url: str,
        project_queue_name: str, workflows: dict, number_of_workers: int, stop_event: Event):
        # nextflow binary
        self.__nf_bin: Path = nf_bin
        # nextflow cloud attributes
        self.__nf_cloud_url: str = nf_cloud_url
        self.__nf_cloud_api_user: str = nf_cloud_api_user
        self.__nf_cloud_api_password: str = nf_cloud_api_password
        self.__project_data_path: Path = projects_data_path
        # message broker attributes
        self.__rabbit_mq_url: str = rabbit_mq_url
        self.__project_queue_name: str = project_queue_name
        self.__connection: Optional[pika.BlockingConnection] = None
        self.__channel: Optional[Channel] = None
        # defined workflows
        self.__workflows: dict = workflows
        # number of workers
        self.__number_of_workers: int = number_of_workers
        # control
        self.__stop_event: Event = stop_event
        

    def start(self):
        """
        Starts the worker
        """
        project_queue: Queue = Queue()
        comm_channels: List[Connection] = []
        wf_executors: List[WorkflowExecutor] = []

        while not self.__stop_event.is_set():
            try:
                self.__connection = pika.BlockingConnection(pika.URLParameters(self.__rabbit_mq_url))
                self.__channel = self.__connection.channel()
                self.__channel.basic_qos(prefetch_count=self.__number_of_workers)

                for _ in range(self.__number_of_workers):
                    ro_comm, rw_comm = Pipe(duplex=False)
                    executor = WorkflowExecutor(
                        self.__nf_bin,
                        self.__nf_cloud_url,
                        self.__nf_cloud_api_user,
                        self.__nf_cloud_api_password,
                        self.__project_data_path,
                        project_queue,
                        self.__workflows,
                        rw_comm,
                        self.__stop_event
                    )
                    executor.start()
                    comm_channels.append(ro_comm)
                    # Close this reference to the read write connection
                    rw_comm.close()
                    wf_executors.append(executor)
                    
                ack_handler: AckHandler = AckHandler(
                    self.__connection,
                    self.__channel,
                    comm_channels
                )
                ack_handler.start()

                # Second return value 'properties' is unnecessary. After 0.5 second it consume will return '(None, None, None)' if no message was send.
                # This will give us time for maintenance, e.g. stop if a stop signal was received
                for method_frame, _, body in self.__channel.consume(self.__project_queue_name, inactivity_timeout=.5, auto_ack=False):
                    if method_frame and body:
                        try:
                            project_queue.put((
                                body,
                                method_frame.delivery_tag
                            ))
                        except FullQueueError:
                            pass
                    if self.__stop_event.is_set():
                        break

            except pika.exceptions.ConnectionWrongStateError as error:
                self.__handle_rabbitmq_connection_error(error)
            except pika.exceptions.ConnectionClosed as error:
                self.__handle_rabbitmq_connection_error(error)
            except pika.exceptions.ChannelWrongStateError as error:
                self.__handle_rabbitmq_connection_error(error)
            except pika.exceptions.ChannelClosed as error:
                self.__handle_rabbitmq_connection_error(error)

        # cancel queued messages
        self.__channel.cancel()
        self.__channel.close()
        self.__connection.close()

    def __handle_rabbitmq_connection_error(self, error: BaseException):
        print(
            "RabbitMQ connetion was closed unexpectedly. Will try to reconnect in a few seconds. Error was:",
            error
        )
        time.sleep(5)
