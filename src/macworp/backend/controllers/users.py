import json
import requests

from fastapi import Depends
from oauthlib.oauth2 import WebApplicationClient
from pydantic import BaseModel
from starlette.responses import JSONResponse

from macworp.backend.auth.abstract_authorization import AbstractAuthorization
from ..auth.login_request import LoginRequest
from ..models.user import User, UserRole
from ..auth.file_based_authorization import FileBasedAuthorization
from ..auth.database_authorization import DatabaseAuthorization
from ..auth.openid_authorization import OpenIDAuthorization
from ..auth.jwt import JWT
from ..auth.provider_type import ProviderType
from ..controllers.depends import DbSession, Configuration

from fastapi import HTTPException, status, APIRouter, Request
from fastapi.responses import RedirectResponse

ACCESS_TOKEN_EXPIRE_SEC = 3600

router = APIRouter(prefix="/users")


class LoginResponse(BaseModel):
    jwt: str


# @router.post("/register/{provider_type}/{provider}", tags=["user"])
# def register_user(user: UserRegisterSchema, provider_type: str, provider: str, db = Depends(get_db)):
#     if provider_type == ProviderType.OPENID_CONNECT.value:

#         if not Authorization.is_user_already_registered(db, user.login_id):
#             db_user =  Authorization.register_new_user(db, user, provider_type, provider)
#             access_token_expires = timedelta(minutes=ACCESS_TOKEN_EXPIRE_MINUTES)
#             token = create_access_token(data={"email": db_user.login_id}, expires_delta=access_token_expires)
#             return {"access_token": token, "token_type": "bearer"}

#         else:
#             raise HTTPException(
#             status_code=404,
#             detail="User is already in registered!",
#             headers={"WWW-Authenticate": "Bearer"},
#             )

#     if provider_type == ProviderType.FILE.value:
#             #Todo
#             pass

#     else:
#         raise HTTPException(status_code=404, detail="Provider Type not found")


@router.get("/login-providers", summary="provides the login Providers")
def login_providers(config: Configuration):
    return JSONResponse(
        {
            str(provider_type): {
                provider_name: provider_config.description
                for provider_name, provider_config in providers
            }
            for provider_type, providers in config.backend.login_providers.iter_providers()
        }
    )


@router.post("/login/{provider_type}/{provider}")
def login_user(
    request: Request,
    provider_type: str,
    provider: str,
    login_request: LoginRequest,
    session: DbSession,
    config: Configuration,
):  # -> LoginResponse | RedirectResponse:
    try:
        provider_type = ProviderType.from_str(provider_type)
    except ValueError as exc:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
            detail="Provider Type not found.",
        ) from exc

    user = None
    match provider_type:
        #     authenticated = Authorization.authenticate_user(db, form_data.username, form_data.password)
        #     if authenticated:
        #         db_user = Authorization.get_user_by_login_id(db, form_data.username)
        #         access_token_expires = timedelta(minutes=ACCESS_TOKEN_EXPIRE_MINUTES)
        #         token = create_access_token(data={"email": db_user.login_id}, expires_delta=access_token_expires)
        #         return {"access_token": token, "token_type": "bearer"}

        #     else:
        #         raise HTTPException(
        #         status_code=status.HTTP_401_UNAUTHORIZED,
        #         detail="Could not validate credentials",
        #         headers={"WWW-Authenticate": "Bearer"},
        #     )
        case ProviderType.OPENID_CONNECT:
            return OpenIDAuthorization.get_redirect(
                request.app, provider, login_request, session, config
            )

        case ProviderType.FILE:
            user = FileBasedAuthorization.login(
                request.app, provider, login_request, session, config
            )

        case ProviderType.DATABASE:
            user = DatabaseAuthorization.login(
                request.app, provider, login_request, session, config
            )

    if user is None:
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail="Could not login, user is still none, please check the provider type and provider name.",
        )

    jwt = JWT.create_auth_token(config.secret, user, ACCESS_TOKEN_EXPIRE_SEC)

    return LoginResponse(jwt=jwt)


@router.get("/login/{provider_type}/{provider}/callback")
def user_auth_callback(
    request: Request,
    provider_type: str,
    provider: str,
    error: str | None,
    code: str,
    session: DbSession,
    config: Configuration,
):

    provider_client_config = config.backend.login_providers.openid.get(provider, None)
    if provider_client_config is None:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
            detail="Provider not supported.",
        )

    #  /api/users/openid/dev/callback?error=unauthorized_client&error_reason=grant_type_disabled&error_description=The+%5Bauthorization_code%5D+Authorization+Code+grant+has+been+disabled+for+this+client
    if error is not None:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_ENTITY, detail=error
        )

    provider_config = OpenIDAuthorization.get_autodiscovery(provider_client_config)
    provider_client = WebApplicationClient(
        config.backend.login_providers.openid[provider].client_id
    )

    # Prepare authorization_token request
    token_url, headers, body = provider_client.prepare_token_request(
        provider_config["token_endpoint"],
        authorization_response=request.url,
        redirect_url=request.base_url,
        code=code,
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
        verify=provider_client_config.backend.get("verify_ssl", True),
    )

    auth_token_data = auth_token_response.json()

    # Parse the tokens!
    provider_client.parse_request_body_response(json.dumps(auth_token_data))

    # Get user profile data from provider
    uri, headers, body = provider_client.add_token(provider_config["userinfo_endpoint"])

    userinfo = requests.get(
        uri,
        headers=headers,
        data=body,
        verify=provider_client_config.backend.get("verify_ssl", True),
    ).json()

    user = session.get(User, userinfo["sub"])
    if user is None:
        user = User(
            login_id=userinfo["sub"],
            email=userinfo["email"],
            role=UserRole.from_str(userinfo["role"]),
            provider_type=ProviderType.OPENID_CONNECT.value,
            provider_name=provider,
            hashed_password=None,
        )
        session.add(user)
        session.commit()

        return user


# @router.post()
# def callback(provider_type: str, provider: str):
#         """
#         Callback for openid login

#         Parameters
#         ----------
#         provider : str
#             Name of provider as indicated in config
#         """
#         if provider_type == ProviderType.OPENID_CONNECT.value:
#             return OpenIdConnectAuthentication.callback(request, provider)
#         if provider_type == ProviderType.FILE.value:
#             return FileBasedAuthentication.callback(request, provider)
#         else:
#             return jsonify({
#                 "errors": {
#                     "general": "Provider type not found."
#                 }
#             }), 404
