# std imports
from enum import unique, Enum

@unique
class ProviderType(Enum):
    """
    Login provider types
    """
    FILE = "file"
    OPENID_CONNECT = "openid"
    DATABASE = "database";

    @classmethod
    def from_str(cls, provider_type: str):
        """
        Convert string to ProviderType

        Parameters
        ----------
        provider_type : str
            Provider type as string
        
        Returns
        -------
        ProviderType
            Provider type

        Raises
        ------
        ValueError
            If provider type is unknown
        """
        match provider_type:
            case cls.FILE.value:
                return cls.FILE
            case cls.DATABASE.value:
                return cls.DATABASE
            case cls.OPENID_CONNECT.value:
                return cls.OPENID_CONNECT
            case _:
                raise ValueError(f"Unknown provider type: {provider_type}")