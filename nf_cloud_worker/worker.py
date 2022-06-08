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
        "__nf_bin",
        "__nf_cloud_url",
        "__rabbit_mq_url",
        "__project_queue",
        "__project_data_path",
        "__workflows",
        "__connection",
        "__channel",
        "__stop_event"
    ]

    __nf_bin: pathlib.Path
    __nf_cloud_url: str
    __rabbit_mq_url: str
    __project_queue: str
    __project_data_path: pathlib.Path
    __workflows: dict
    __connection: pika.BaseConnection
    __channel: Channel
    __stop_event: Event

    def __init__(self, nf_bin: pathlib.Path, nf_cloud_url: str, rabbit_mq_url: str, workflow_queue: str, workflow_data_path: pathlib.Path, workflows: dict, stop_event: Event):
        self.__nf_bin = nf_bin
        self.__nf_cloud_url = nf_cloud_url
        self.__rabbit_mq_url = rabbit_mq_url
        self.__project_queue = workflow_queue
        self.__project_data_path = workflow_data_path
        self.__workflows = workflows
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
                for method_frame, _, body in self.__channel.consume(self.__project_queue, inactivity_timeout=self.__class__.INACTIVITY_TIMEOUT):
                    if method_frame and body:
                        try:
                            # Parse identification arguments
                            project_params = json.loads(body)
                            print("project_params:", project_params)

                            work_dir = self.__project_data_path.joinpath(f"{project_params['id']}/")
                            weblog_url = f"{self.__nf_cloud_url}/api/projects/{project_params['id']}/workflow-log"
                            workflow = None
                            workflow = Workflow(
                                self.__nf_bin,
                                work_dir,
                                self.__get_workflow_path(
                                    project_params["workflow"]
                                ),
                                self.__get_workflow_script_path(
                                    project_params["workflow"]
                                ),
                                self.__get_direct_nextflow_parameters(
                                    project_params["workflow"]
                                ),
                                project_params["workflow_arguments"],
                                self.__get_workflow_static_arguments(
                                    project_params["workflow"]
                                ),
                                weblog_url
                            )
                            workflow.start()
                            self.__channel.basic_ack(delivery_tag = method_frame.delivery_tag)
                        finally:
                            requests.post(f"{self.__nf_cloud_url}/api/projects/{project_params['id']}/finished")
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

    def __get_workflow_path(self, workflow: str) -> pathlib.Path:
        directory = self.__workflows[workflow]["directory"]
        return pathlib.Path(directory).absolute()

    def __get_workflow_script_path(self, workflow: str) -> str:
        return self.__workflows[workflow]["script"]

    def __get_workflow_static_arguments(self, workflow: str) -> str:
        if "static" in self.__workflows[workflow]["args"]:
            return self.__workflows[workflow]["args"]["static"]
        return {}

    def __get_direct_nextflow_parameters(self, workflow: str) -> list:
        if "nextflow_parameters" in self.__workflows[workflow]:
            return self.__workflows[workflow]["nextflow_parameters"]
        return []


