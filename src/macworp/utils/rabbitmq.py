"""Some functions to control RabbitMQ"""

import pika
import time
import traceback
from typing import Tuple


class RabbitMQ:
    @staticmethod
    def init_queues(rabbit_mq_url: str, queue_name: str) -> None:
        """
        Prepares the message queues if necessary
        """
        while True:
            try:
                # Establish connection
                connection = pika.BlockingConnection(pika.URLParameters(rabbit_mq_url))
                channel = connection.channel()

                # Create main queue
                channel.queue_declare(
                    queue=queue_name,
                    durable=True,
                )

                break
            except pika.exceptions.AMQPConnectionError:
                app.logger.warning(
                    "Cannot connect to RabbitMQ for configuration. Try it again in 2 seconds."
                )  # pylint: disable=no-member
                time.sleep(2)
                continue
            except:
                raise BaseException(traceback.format_exc())

    @staticmethod
    def get_queue_statistics(rabbit_mq_url: str, queue_name: str) -> Tuple[int, int]:
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
            connection = pika.BlockingConnection(pika.URLParameters(rabbit_mq_url))
            channel = connection.channel()

            # Get queue statistics
            queue_state = channel.queue_declare(
                queue=queue_name,
                durable=True,
                passive=True,
            )
            connection.close()

            return queue_state.method.consumer_count, queue_state.method.message_count
        except:
            return None
