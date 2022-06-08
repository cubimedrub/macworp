# std importd
import json
import time
from threading import Thread
import traceback

# 3rd party imports
import eventlet
from flask import Flask, g as request_store, request
from flask_cors import CORS
from flask_socketio import SocketIO
from playhouse.flask_utils import FlaskDB
from werkzeug.exceptions import HTTPException

# internal imports
from nf_cloud_backend.utility.configuration import Configuration, Environment
from nf_cloud_backend.utility.headers.cross_origin_resource_sharing import add_allow_cors_headers
from nf_cloud_backend.utility.matomo import track_request as matomo_track_request

# Load config and environment.
config, env = Configuration.get_config_and_env()

app = Flask('app')
"""Flask app
"""

# Allow CORS in general
CORS(app)

# Default Flask parameter
app.config.update(
    ENV = env.name,
    DEBUG = config['debug'],
    SECRET_KEY = bytes(config['secret'], "ascii"),
    PREFERRED_URL_SCHEME = 'https' if config['use_https'] else 'http'
)

async_mode = "threading"
"""Mode for SocketIO
"""
if env == Environment.production:
    eventlet.monkey_patch()
    async_mode = "eventlet"

# SocketIO for bi-directional 
socketio = SocketIO(app, cors_allowed_origins="*")


socketio = SocketIO(
    app,
    message_queue=config['rabbit_mq']['url'],
    cors_allowed_origins="*",
    async_mode=async_mode,
    engineio_logger=app.logger if config['debug'] else False,
    logger=config['debug'],
    always_connect=True
)
"""SocketIO for bidirectional communication (events) between server and browser.
"""

db_wrapper = FlaskDB(
    app,
    config['database']['url']
)

@app.before_request
def track_request():
    """
    Sends a tracking request to Matomo.
    """
    if config["matomo"]["enabled"]:
        track_thread = Thread(target=matomo_track_request, args=(
            request.headers.get("User-Agent", ""),
            request.remote_addr,
            request.headers.get("Referer", ""),
            request.headers.get("Accept-Language", ""),
            request.headers.get("Host", ""),
            request.full_path,
            request.query_string,
            request.url.startswith("https"),
            config["matomo"]["url"],
            config["matomo"]["site_id"],
            config["matomo"]["auth_token"], 
            app,
            config["debug"]
        ))
        track_thread.start()
        request_store.track_thread = track_thread 

@app.teardown_appcontext
def wait_for_track_request(exception=None): #pylint: disable=unused-argument
    """
    Waits for the Matomo tracking to finish.s

    Parameters
    ----------
    exception : Any, optional
        Required for `
    """
    track_thread = request_store.pop("track_thread", None)
    if track_thread:
        track_thread.join()

@app.errorhandler(Exception)
def handle_exception(e):
    """
    Catching exceptions and sends them as requests.

    Parameters
    ----------
    e : Any
        Necessary for errorhandler

    Returns
    -------
    Response
        Response with formatted exception
    """
    response = None
    # pass through HTTP errors
    if isinstance(e, HTTPException):
        # Return JSON instead of HTML for HTTP errors
        # start with the correct headers and status code from the error
        response = e.get_response()
        # replace the body with JSON
        response.data = json.dumps({
            "errors": {
                "general": e.description
            }
        })
        response.content_type = "application/json"
    else:
        response = app.response_class(
            response=json.dumps({
                "errors": {
                    "general": str(e)
                }
            }),
            status=500,
            mimetype='application/json'
        )
    if config['debug']:
        app.logger.error(traceback.format_exc()) # pylint: disable=no-member
        response = add_allow_cors_headers(response)
    return response

# Do not move this the top of the file, cause the modeule imports some values which are set during the initialization.
from nf_cloud_backend.utility.rabbit_mq import RabbitMQ # pylint: disable=wrong-import-position
RabbitMQ.prepare_queues()

# Import controllers.
# Do not move this import to the top of the files. Each controller uses 'app' to build the routes.
# Some controllers also import the connection pools.
from .controllers import *  # pylint: disable=wrong-import-position