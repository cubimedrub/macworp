# std importd
import json
from threading import Thread
import traceback
from typing import Optional, Tuple

# 3rd party imports
import eventlet
from flask import Flask, g as request_store, request, Request
from flask_caching import Cache
from flask_cors import CORS
from flask_login import LoginManager
from flask_socketio import SocketIO
import jwt
from oauthlib.oauth2 import WebApplicationClient
from playhouse.flask_utils import FlaskDB
from werkzeug.middleware.proxy_fix import ProxyFix
from werkzeug.exceptions import HTTPException

# internal imports
from nf_cloud_backend.constants import (
    ACCESS_TOKEN_HEADER, 
    ONE_TIME_USE_ACCESS_TOKEN_PARAM_NAME,
    ONE_TIME_USE_ACCESS_TOKEN_CACHE_PREFIX
)
from nf_cloud_backend.utility.configuration import Configuration
from nf_cloud_backend.utility.headers.cross_origin_resource_sharing import add_allow_cors_headers
from nf_cloud_backend.utility.matomo import track_request as matomo_track_request

from nf_cloud_backend import models   # Import module only to prevent circular imports

# Load config and environment.
Configuration.initialize()

app = Flask('app')
"""Flask app
"""

# Allow CORS in general
CORS(app)

# Default Flask parameter
app.config.update(
    ENV = "development" if Configuration.values()["debug"] else "production",
    DEBUG = Configuration.values()['debug'],
    SECRET_KEY = Configuration.values()['secret']
)

if Configuration.values()["use_reverse_proxy"]:
    app.wsgi_app = ProxyFix(
        app.wsgi_app,
        x_for=1,
        x_proto=1,
        x_host=1,
        x_prefix=1
    )

cache = Cache(
    config={
        "CACHE_TYPE": "SimpleCache",
        "CACHE_DEFAULT_TIMEOUT": 60
    }
)
""" Cache for one time use authentication tokens
"""
cache.init_app(app)

async_mode = "threading"
"""Mode for SocketIO
"""
if not Configuration.values()["debug"]:
    eventlet.monkey_patch()
    async_mode = "eventlet"

# SocketIO for bi-directional 
socketio = SocketIO(app, cors_allowed_origins="*")


socketio = SocketIO(
    app,
    message_queue=Configuration.values()['rabbit_mq']['url'],
    cors_allowed_origins="*",
    async_mode=async_mode,
    engineio_logger=app.logger if Configuration.values()['debug'] else False,
    logger=Configuration.values()['debug'],
    always_connect=True
)
"""SocketIO for bidirectional communication (events) between server and browser.
"""

db_wrapper = FlaskDB(
    app,
    Configuration.values()['database']['url']
)

openid_clients = {
    provider: WebApplicationClient(provider_data["client_id"])
    for provider, provider_data in Configuration.values()["login_providers"]["openid"].items()
}

login_manager = LoginManager()
login_manager.init_app(app)

# Do not move import up, it would result in cyclic dependencies
from nf_cloud_backend.authorization.jwt import JWT                      # pylint: disable=wrong-import-position

@login_manager.request_loader
def load_user_from_request(incomming_request: Request):
    """
    Get user by Authorization-header.

    Parameters
    ----------
    incomming_request : Request
        Incomming request

    Returns
    -------
    User : optional
    """

    # Check if access token is provided in header
    auth_header = incomming_request.headers.get(ACCESS_TOKEN_HEADER, None)
    # If the JWT token isn't found in the header, try to resolve the JWT token by an one time use token
    # These are usually used for download URLs via the frontend where it is not possible to
    # add the access token to the headers and the file is too large fo the usual "Download -> Blob -> Blob download"-Javascript stuff.
    if auth_header is None:
        one_time_use_token: Optional[str] = incomming_request.args.get(ONE_TIME_USE_ACCESS_TOKEN_PARAM_NAME, None)
        one_time_use_token = f"{ONE_TIME_USE_ACCESS_TOKEN_CACHE_PREFIX}{one_time_use_token}"
        if cache.has(one_time_use_token):
            auth_header = cache.get(one_time_use_token)
            # Delete the one time use token from cache
            cache.delete(one_time_use_token)
        else:
            auth_header = None
    if auth_header is not None:
        try:
            user, is_unexpired = JWT.decode_auth_token_to_user(
                app.config["SECRET_KEY"],
                auth_header
            )
            if user is not None and is_unexpired:
                return user
        except (
            jwt.ExpiredSignatureError,
            jwt.InvalidTokenError
        ):
            return None
    auth_header = incomming_request.headers.get("Authorization", None)
    if auth_header is not None:
        basic_auth = incomming_request.authorization
        if basic_auth.username == Configuration.values()["worker_credentials"]["username"] \
            and basic_auth.password == Configuration.values()["worker_credentials"]["password"]:
            return models.user.User(
                id=0,
                provider_type="local",
                provider="local",
                login_id="worker"
            )
    return None

@app.before_request
def track_request():
    """
    Sends a tracking request to Matomo.
    """
    if Configuration.values()["matomo"]["enabled"]:
        track_thread = Thread(target=matomo_track_request, args=(
            request.headers.get("User-Agent", ""),
            request.remote_addr,
            request.headers.get("Referer", ""),
            request.headers.get("Accept-Language", ""),
            request.headers.get("Host", ""),
            request.full_path,
            request.query_string,
            request.url.startswith("https"),
            Configuration.values()["matomo"]["url"],
            Configuration.values()["matomo"]["site_id"],
            Configuration.values()["matomo"]["auth_token"], 
            app,
            Configuration.values()["debug"]
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
    if Configuration.values()['debug']:
        app.logger.error(traceback.format_exc()) # pylint: disable=no-member
        response = add_allow_cors_headers(response)
    return response



# Import controllers.
# Do not move this import to the top of the files. Each controller uses 'app' to build the routes.
# Some controllers also import the connection pools.
from .controllers import *  # pylint: disable=wrong-import-position