# std imports
import json
import pathlib
import time
from dataclasses import dataclass
from typing import ClassVar
from threading import Event

# external imports
import pika
import requests
from pika.channel import Channel

# internal imports
from nf_cloud_worker.workflows.workflow import Workflow

@dataclass
class Worker:
    """
    Connects to workflow message queue, fetches messages and starts workflow.
    """

    PREFETCH_COUNT: ClassVar[int] = 1
    """Number of messages to prefetch from the queue.
    """

    INACTIVITY_TIMEOUT: ClassVar[int] = 5
    """Timeout for getting messages
    """

    __slots__ = [
        "__nf_cloud_url",
        "__rabbit_mq_url",
        "__workflow_queue",
        "__workflow_data_path",
        "__nextflow_workflows",
        "__connection",
        "__channel",
        "__stop_event"
    ]

    __nf_cloud_url: str
    __rabbit_mq_url: str
    __workflow_queue: str
    __workflow_data_path: pathlib.Path
    __nextflow_workflows: dict
    __connection: pika.BaseConnection
    __channel: Channel
    __stop_event: Event

    def __init__(self, nf_cloud_url: str, rabbit_mq_url: str, workflow_queue: str, workflow_data_path: pathlib.Path, nextflow_workflows: dict, stop_event: Event):
        self.__nf_cloud_url = nf_cloud_url
        self.__rabbit_mq_url = rabbit_mq_url
        self.__workflow_queue = workflow_queue
        self.__workflow_data_path = workflow_data_path
        self.__nextflow_workflows = nextflow_workflows
        self.__connection = None
        self.__channel = None
        self.__stop_event = stop_event

    def start(self):
        while not self.__stop_event.is_set():
            try:
                self.__connection = pika.BlockingConnection(pika.URLParameters(self.__rabbit_mq_url))
                self.__channel = self.__connection.channel()
                self.__channel.basic_qos(prefetch_count=self.__class__.PREFETCH_COUNT)

                # Second return value 'properties' is unnecessary. After 5 second it consume will return '(None, None, None)' if no message was send.
                # This will give us time for maintenance, e.g. stop if a stop signal was received
                for method_frame, _, body in self.__channel.consume(self.__workflow_queue, inactivity_timeout=self.__class__.INACTIVITY_TIMEOUT):
                    if method_frame and body:
                        try:
                            # Parse identification arguments
                            workflow_params = json.loads(body)
                            print("workflow_params:", workflow_params)

                            work_dir = self.__workflow_data_path.joinpath(f"{workflow_params['id']}/")
                            weblog_url = f"{self.__nf_cloud_url}/api/workflows/{workflow_params['id']}/nextflow-log"
                            workflow = None
                            workflow = Workflow(
                                work_dir,
                                self.__get_nextflow_workflow_path(
                                    workflow_params["nextflow_workflow"]
                                ),
                                self.__get_nextflow_workflow_script_path(
                                    workflow_params["nextflow_workflow"]
                                ),
                                self.__get_direct_nextflow_parameters(
                                    workflow_params["nextflow_workflow"]
                                ),
                                workflow_params["nextflow_arguments"],
                                self.__get_nextflow_workflow_static_arguments(
                                    workflow_params["nextflow_workflow"]
                                ),
                                weblog_url
                            )
                            workflow.start()
                            self.__channel.basic_ack(delivery_tag = method_frame.delivery_tag)
                        finally:
                            requests.post(f"{self.__nf_cloud_url}/api/workflows/{workflow_params['id']}/finished")
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
            # finally:
            #     if log_file and not log_file.closed:
            #         log_file.close()
        
        # cancel queued messages
        self.__channel.cancel()     
        self.__channel.close()
        self.__connection.close()

    def __handle_rabbitmq_connection_error(self, error: BaseException):
        print("RabbitMQ connetion was closed unexpectedly. Will try to reconnect in a few seconds. Error was:", error)
        time.sleep(5)

    def __get_nextflow_workflow_path(self, nextflow_workflow: str) -> pathlib.Path:
        directory = self.__nextflow_workflows[nextflow_workflow]["directory"]
        return pathlib.Path(directory).absolute()

    def __get_nextflow_workflow_script_path(self, nextflow_workflow: str) -> str:
        return self.__nextflow_workflows[nextflow_workflow]["script"]

    def __get_nextflow_workflow_static_arguments(self, nextflow_workflow: str) -> str:
        if "static" in self.__nextflow_workflows[nextflow_workflow]["args"]:
            return self.__nextflow_workflows[nextflow_workflow]["args"]["static"]
        return {}

    def __get_direct_nextflow_parameters(self, nextflow_workflow: str) -> list:
        if "nextflow_parameters" in self.__nextflow_workflows[nextflow_workflow]:
            return self.__nextflow_workflows[nextflow_workflow]["nextflow_parameters"]
        return []


