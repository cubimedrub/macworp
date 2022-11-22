import pika
import time
import traceback
from typing import Tuple

from nf_cloud_backend import app
from nf_cloud_backend.utility.configuration import Configuration

class RabbitMQ:
    @staticmethod
    def prepare_queues():
        """
        Prepares the message queues if necessary
        """
        while True:
            try:
                # Establish connection
                connection = pika.BlockingConnection(pika.URLParameters(Configuration.values()['rabbit_mq']['url'] ))
                channel = connection.channel()

                # Create main queue
                channel.queue_declare(
                    queue=Configuration.values()['rabbit_mq']['project_workflow_queue'], 
                    durable=True
                )

                break
            except pika.exceptions.AMQPConnectionError:
                app.logger.warning("Cannot connect to RabbitMQ for configuration. Try it again in 2 seconds.") # pylint: disable=no-member
                time.sleep(2)
                continue
            except:
                raise BaseException(traceback.format_exc())

    @staticmethod
    def get_queue_statistics(queue: str) -> Tuple[int, int]:
        """
        Get the consumer and message count of the given queue.

        Parameters
        ----------
        queue : str
            Queue name

        Returns
        -------
        Returns a tuple with consumer count and message count
        """
        try:
            # Establish connection
            connection = pika.BlockingConnection(pika.URLParameters(Configuration.values()['rabbit_mq']['url'] ))
            channel = connection.channel()

            # Get queue statistics
            queue_state = channel.queue_declare(
                queue=Configuration.values()['rabbit_mq']['project_workflow_queue'], 
                durable=True,
                passive=True
            )
            connection.close()

            return queue_state.method.consumer_count, queue_state.method.message_count
        except:
            return None
