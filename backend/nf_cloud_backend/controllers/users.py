from pydantic import BaseModel
from ..auth.login_request import LoginRequest

from ..auth.file_based_authorization import FileBasedAuthorization
from ..auth.database_authorization import DatabaseAuthorization
from ..auth.openid_authorization import OpenIDAuthorization
from ..auth.jwt import JWT
from ..auth.provider_type import ProviderType
from ..configuration import SECRET_KEY
from ..controllers.depends import DbSession

from fastapi import HTTPException, status, APIRouter

ACCESS_TOKEN_EXPIRE_SEC = 3600

router = APIRouter(
    prefix="/users"
)

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

@router.post("/login/{provider_type}/{provider}")
def login_user(provider_type: str, provider: str, login_request: LoginRequest, session: DbSession):
    try:
        type = ProviderType.from_str(provider_type)
    except ValueError as exc:
        raise HTTPException(status_code=status.HTTP_422_UNPROCESSABLE_ENTITY, detail="Provider Type not found.") from exc

    user = None
    match type:
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
            user = OpenIDAuthorization.login(provider, login_request, session)
            
        case ProviderType.FILE:
            user = FileBasedAuthorization.login(provider, login_request, session)

        case ProviderType.DATABASE:
            user = DatabaseAuthorization.login(provider, login_request, session)

    if user is None:
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail="Could not login, user is still none, please check the provider type and provider name.",
        )
    
    jwt = JWT.create_auth_token(SECRET_KEY, user, ACCESS_TOKEN_EXPIRE_SEC)

    return LoginResponse(jwt=jwt)

