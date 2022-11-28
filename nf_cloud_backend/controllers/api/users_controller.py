# std imports
import secrets

# 3rd party imports
from flask import jsonify, request
from flask_login import login_required, logout_user

# internal imports
from nf_cloud_backend import app, cache
from nf_cloud_backend.authorization.provider_type import ProviderType
from nf_cloud_backend.authorization.jwt import JWT
from nf_cloud_backend.authorization.openid_connect import OpenIdConnect
from nf_cloud_backend.constants import ACCESS_TOKEN_HEADER, ONE_TIME_USE_ACCESS_TOKEN_CACHE_PREFIX
from nf_cloud_backend.utility.configuration import Configuration


class UsersController:
    """
    Controller for user management.
    """

    @staticmethod
    @app.route("/api/users/login-providers")
    def login_providers():
        """
        Return the given login providers

        Returns
        -------
        Reponse
        """
        return jsonify({
            provider_type: {
                provider: values.get("description", "No desription provided") 
                    for provider, values in Configuration.values()["login_providers"][provider_type].items()
            } for provider_type in Configuration.values()["login_providers"]
        })

    @staticmethod
    @app.route('/api/users/<string:provider_type>/<string:provider>/login')
    def login(provider_type: str, provider: str):
        """
        Login for openid provider. Response contains JWT token and JWT timeout.

        Parameters
        ----------
        provider : str
            Name of provider as indicated in config

        Returns
        -------
        Respnse
        """
        if provider_type == ProviderType.OPENID_CONNECT.value:
            return OpenIdConnect.login(request, provider)
        else:
            return jsonify({
                "errors": {
                    "general": "Provider type not found."
                }
            }), 404

    @staticmethod
    @app.route('/api/users/<string:provider_type>/<string:provider>/callback', endpoint="user_auth_callback")
    def callback(provider_type: str, provider: str):
        """
        Callback for openid login

        Parameters
        ----------
        provider : str
            Name of provider as indicated in config
        """
        if provider_type == ProviderType.OPENID_CONNECT.value:
            return OpenIdConnect.callback(request, provider)
        else:
            return jsonify({
                "errors": {
                    "general": "Provider type not found."
                }
            }), 404

    @staticmethod
    @app.route("/api/users/logout")
    @login_required
    def logout():
        """
        Logout for users

        Returns
        -------
        Response
        """
        logout_user()
        return "", 200

    @staticmethod
    @app.route("/api/users/logged-in")
    @login_required
    def logged_in():
        """
        Checks if token is not expired.

        Returns
        -------
        Response
            200 if expired
            401 if expired
        """
        auth_header = request.headers.get(ACCESS_TOKEN_HEADER, None)
        if auth_header is None:
            return jsonify({
                "errors": {
                    "general": "No authorization token provided."
                }
            }), 401

        user, is_unexpired = JWT.decode_auth_token_to_user(
            app.config["SECRET_KEY"],
            auth_header
        )
        if user is not None:
            if is_unexpired:
                return "", 200
            else:
                if not is_unexpired:
                    try:
                        if user.provider == ProviderType.OPENID_CONNECT.value:
                            return OpenIdConnect.refresh_token(request, user)
                    except KeyError:
                        pass
        return jsonify({"errors": {
            "general": "login is expired"
        }}), 401


    @staticmethod
    @app.route("/api/users/one-time-use-token")
    @login_required
    def one_time_use_token():
        """
        Generates an authentication token for one time use.
        The token can be used via an URL query parameter, when it is not possible
        to add the JWT token to the header, e.g. for download links.

        Returns
        -------
        Response
            200
        """
        original_auth_token: str = request.headers.get(ACCESS_TOKEN_HEADER, None)
        while True:
            one_time_use_token: str = secrets.token_hex(32)
            if cache.has(one_time_use_token):
                continue
            # Put the password and JWT token for ten seconds in the cache
            cache.set(
                f"{ONE_TIME_USE_ACCESS_TOKEN_CACHE_PREFIX}{one_time_use_token}",
                original_auth_token,
                timeout=10
            )
            return jsonify({
                "token": one_time_use_token
            })
