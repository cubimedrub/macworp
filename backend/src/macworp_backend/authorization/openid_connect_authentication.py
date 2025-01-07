# std imports
import datetime
import json
from typing import Any, ClassVar, Dict

# 3rd party imports
from flask import Request, jsonify, redirect, Response, url_for
from flask_login import login_user
import requests

# internal imports
from macworp_backend import openid_clients, app
from macworp_backend.authorization.provider_type import ProviderType
from macworp_backend.authorization.jwt import JWT
from macworp_backend.models.user import User
from macworp_backend.authorization.abstract_authentication import AbstractAuthentication


class OpenIdConnectAuthentication(AbstractAuthentication):
    """
    Tools to manage OpenID authentications
    """

    PROVIDER_TYPE: ClassVar[ProviderType] = ProviderType.OPENID_CONNECT

    @classmethod
    def get_autodicovery(cls, provider_client_config: Dict[str, Any]) -> Dict[Any, Any]:
        """
        Get the autodiscovery.

        Parameters
        ----------
        provider_config : Dict[str, Any]
            Provider specific config from application config

        Returns
        -------
        Dict[Any, Any]
            Provider config
        """
        response = requests.get(
            provider_client_config["discovery_url"],
            verify=provider_client_config.get("verify_ssl", True),
            timeout=60,
        )
        try:
            return response.json()
        except Exception as err:
            raise ValueError(
                (
                    f"Could not decode JSON from OpenID connect provider discovery endpoint\n"
                    f"\tURL: {provider_client_config['discovery_url']}\n"
                    f"\tStatus code: {response.status_code}\n"
                    f"\tBody: '{response.text}'"
                )
            ) from err

    @classmethod
    def login(cls, request: Request, provider: str) -> Response:
        provider_client_config = cls.get_provider_client_config(provider)
        if provider_client_config is None:
            return jsonify({"errors": {"general": "Provider not supported."}}), 400

        provider_config = cls.get_autodicovery(provider_client_config)
        provider_client = openid_clients[provider]

        redirect_uri: str = url_for(
            "user_auth_callback",
            _external=True,
            _scheme=request.scheme,
            provider_type=ProviderType.OPENID_CONNECT.value,
            provider=provider,
        )

        # Use library to construct the request for Google login and provide
        # scopes that let you retrieve user's profile from Google
        request_uri = provider_client.prepare_request_uri(
            provider_config["authorization_endpoint"],
            redirect_uri=redirect_uri,
            scope=[provider_client_config.get("scope", "openid"), "email"],
        )
        return redirect(request_uri)

    @classmethod
    def callback(cls, request: Request, provider: str) -> Response:
        """
        Handles the callback from the provider.

        Parameters
        ----------
        request : Request
            Provider callback request
        provider : str
            Provider name

        Returns
        -------
        Response
            Redirect to frontend root URL with JWT token as query parameter in the location url (header `Location`)
        """
        provider_client_config = cls.get_provider_client_config(provider)
        if provider_client_config is None:
            return jsonify({"errors": {"general": "Provider not supported."}}), 400

        #  /api/users/openid/dev/callback?error=unauthorized_client&error_reason=grant_type_disabled&error_description=The+%5Bauthorization_code%5D+Authorization+Code+grant+has+been+disabled+for+this+client
        if request.args.get("error", None) is not None:
            return (
                jsonify(
                    {"errors": {key: value for key, value in request.args.items()}}
                ),
                500,
            )

        provider_config = cls.get_autodicovery(provider_client_config)
        provider_client = openid_clients[provider]

        auth_code = request.args.get("code")

        # Prepare authorization_token request
        token_url, headers, body = provider_client.prepare_token_request(
            provider_config["token_endpoint"],
            authorization_response=request.url,
            redirect_url=request.base_url,
            code=auth_code,
            scope=provider_client_config["scope"],
        )

        auth_token_response = requests.post(
            token_url,
            headers=headers,
            data=body,
            auth=(
                provider_client_config["client_id"],
                provider_client_config["client_secret"],
            ),
            verify=provider_client_config.get("verify_ssl", True),
        )

        auth_token_data = auth_token_response.json()

        # Parse the tokens!
        provider_client.parse_request_body_response(json.dumps(auth_token_data))

        # Get user profile data from provider
        uri, headers, body = provider_client.add_token(
            provider_config["userinfo_endpoint"]
        )

        userinfo = requests.get(
            uri,
            headers=headers,
            data=body,
            verify=provider_client_config.get("verify_ssl", True),
        ).json()

        user = (
            User.select()
            .where(
                User.provider_type == "openid",
                User.provider == provider,
                User.login_id == userinfo["sub"],
            )
            .get_or_none()
        )
        if user is None:
            user = User.create(
                provider_type="openid",
                provider=provider,
                login_id=userinfo["sub"],
                email=userinfo["email"],
            )
        user.provider_data = auth_token_data
        user.save()

        login_user(user)

        token = JWT.create_auth_token(
            app.config["SECRET_KEY"],
            user,
            auth_token_data["expires_in"] + int(datetime.datetime.utcnow().timestamp()),
        )

        return cls.redirect_to_frontend_callback(request, token)

    @classmethod
    def logout(cls, user: User) -> Response:
        return (
            jsonify(
                {
                    "errors": {
                        "general": "Logout is currently not supported for OpenID Connect."
                    }
                }
            ),
            501,
        )

    @classmethod
    def refresh_token(cls, request: Request, user: User) -> Response:
        """
        Refreshes the JWT token.

        Parameters
        ----------
        request : Request
            Request
        user : User
            User to refresh

        Returns
        -------
        Response
            Redirect to frontend root URL with JWT token as query parameter
        """
        if not "refresh_token" in user.provider_data:
            return KeyError("no refresh token found")

        provider_client_config = cls.get_provider_client_config(user.provider)
        if provider_client_config is None:
            return jsonify({"errors": {"general": "Provider not supported."}}), 400

        provider_config = cls.get_autodicovery(provider_client_config)
        provider_client = openid_clients[user.provider]

        refresh_url, refresh_headers, refresh_body = (
            provider_client.prepare_refresh_token_request(
                provider_config["token_endpoint"],
                refresh_token=user.provider_data["refresh_token"],
            )
        )

        refreshed_data = requests.post(
            refresh_url,
            headers=refresh_headers,
            data=refresh_body,
            auth=(
                provider_client_config["client_id"],
                provider_client_config["client_secret"],
            ),
            verify=provider_client_config.get("verify_ssl", True),
        ).json()

        auth_token = JWT.create_auth_token(
            app.config["SECRET_KEY"],
            user,
            int(datetime.datetime.utcnow().timestamp()) + refreshed_data["expires_in"],
        )

        user.provider_data = refreshed_data
        user.save()

        return cls.redirect_to_frontend_callback(request, auth_token)
