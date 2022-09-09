# std imports
from enum import unique, Enum

@unique
class ProviderType(Enum):
    """
    Login provider types
    """
    LOCAL = "local"
    OPENID_CONNECT = "openid"