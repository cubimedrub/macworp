# std imports
from enum import unique, Enum

@unique
class ProviderType(Enum):
    """
    Login provider types
    """
    FILE = "file"
    OPENID_CONNECT = "openid"