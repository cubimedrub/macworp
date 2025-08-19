"""Module for configuration"""

from __future__ import annotations
from pathlib import Path
from typing import Dict, Iterator, List, Optional, Self, Tuple

from pydantic import BaseModel
from pydantic_yaml import parse_yaml_raw_as, to_yaml_str

from macworp.backend.auth.provider_type import ProviderType


class MatomoConfiguration(BaseModel):
    """
    Configuration for Matomo
    """

    enabled: bool = False
    url: str = ""
    site_id: int = 1
    auth_token: str = ""


class RabbitMQConfiguration(BaseModel):
    """
    Configuration for RabbitMQ
    """

    url: str = "amqp://admin:developer@127.0.0.1:5674/%2f"
    project_workflow_queue: str = "project_workflow"


class Credential(BaseModel):
    """
    Credentials for worker
    """

    username: str = "worker"
    password: str = "developer"


class LoginProviderConfiguration(BaseModel):
    description: str


class OpenIdProviderConfiguration(LoginProviderConfiguration):
    """
    Configuration for OpenID provider
    """

    description: str = "Login with OpenID"

    client_id: str = "c63f35e8-66dc-4ec3-bb34-b29803ba72f1"
    """Client ID for OpenID provider"""

    client_secret: str = (
        "8CLrA9gAVmlM7GKKCnMnT9MHNvUMqXNVWog2kWUuC7Y2iBOnFIh9X0rpCqfMcpfQkEZfCBfxMqfz5FYC6cduB20kxqV5Ysq"
    )
    """Client secret for OpenID provider"""

    discovery_url: str = (
        "http://localhost:9011/31a047c3-3353-fcd4-28e0-965a473a1ee8/.well-known/openid-configuration"
    )
    """Discovery URL for OpenID provider"""

    scope: Optional[str] = "offline_access"
    """Scope for OpenID provider"""

    verify_ssl: bool = False


class FileLoginProviderConfiguration(LoginProviderConfiguration):
    """
    Configuration for file login provider
    """

    description: str = "Login with local file"

    file: Path = Path("./dev.users.yml")
    """Path to the file containing user credentials in YAML format."""

    expires_in: int = 4294967296  # 2^32 seconds, ~136 years
    """Expiration time for the login in seconds. Default is 2^32 seconds (~136 years)."""


class DatabaseLoginProviderConfiguration(LoginProviderConfiguration):
    """
    Configuration for database login provider which uses the local database
    """

    description: str = "Login with local database"


class LoginProviderCollection(BaseModel):
    """
    Collection of login providers
    """

    database: Optional[DatabaseLoginProviderConfiguration] = (
        DatabaseLoginProviderConfiguration()
    )
    """Database login provider configuration"""

    openid: Optional[Dict[str, OpenIdProviderConfiguration]] = {
        "dev": OpenIdProviderConfiguration()
    }
    """OpenID login provider configurations"""

    file: Optional[Dict[str, FileLoginProviderConfiguration]] = {
        "dev": FileLoginProviderConfiguration()
    }
    """File login provider configurations"""

    def iter_providers(
        self,
    ) -> Iterator[Tuple[ProviderType, List[Tuple[str, LoginProviderConfiguration]]]]:
        """Returns the login providers as an iterator.

        Yields
        ------
        Iterator[Tuple[ProviderType, List[Tuple[str, LoginProviderConfiguration]]]]
            An iterator over tuples containing the provider type and a list of tuples
            with provider name and its configuration.
        """

        if self.database is not None:
            yield (ProviderType.DATABASE, [("local", self.database)])

        if self.openid is not None:
            yield (
                ProviderType.OPENID_CONNECT,
                [
                    (provider_name, provider_config)
                    for provider_name, provider_config in self.openid.items()
                ],
            )

        if self.file is not None:
            yield (
                ProviderType.FILE,
                [
                    (provider_name, provider_config)
                    for provider_name, provider_config in self.file.items()
                ],
            )


class BackendConfiguration(BaseModel):
    """
    MaCWorP configuration.
    """

    interface: str = "127.0.0.1"
    """Interface to use (0.0.0.0 = all interfaces)"""

    port: int = 3001
    """# Port to use"""

    use_reverse_proxy: bool = False
    """Set to true if you use a reverse proxy"""

    upload_path: str = "./uploads"
    """Path to put working directories"""

    database: str = "postgresql+psycopg://postgres:developer@127.0.0.1:5434/macworp"
    """Database URL, e.g. postgresql+psycopg://user:password@host:port/database"""

    matomo: MatomoConfiguration = MatomoConfiguration()
    """Matomo configuration"""

    rabbitmq: RabbitMQConfiguration = RabbitMQConfiguration()
    """RabbitMQ configuration"""

    redis_url: str = "redis://localhost:6380/0"
    """Redis URL, e.g. redis://host:port/db"""

    worker_credentials: Credential = Credential()
    """Credentials for worker"""

    login_providers: LoginProviderCollection = LoginProviderCollection()
    """Collection of login providers"""

    frontend_host_url: Optional[str] = "http://localhost:5001"
    """
    Redirect after successful login.
    Can be null when frontend and backend using the same domain and port
    """

    allowed_origins: list[str] = ["http://localhost:3001", "http://127.0.0.1:3001"]
    """
    List of allowed origins for CORS requests. 
    This is used to allow the frontend to access the backend
    """


class FrontendConfiguration(BaseModel):
    """Fronend configuration"""

    app_name: str = "MAcWorP"
    debug: bool = False
    host: str = "0.0.0.0"
    port: int = 8080

    backend_url: str = "http://localhost:3001"
    api_version: str = "v1"

    dark_mode: bool = True
    theme_color: str = "#1976d2"

    session_timeout: int = 3600


class Configuration(BaseModel):
    """MAcWorP configuration"""

    backend: BackendConfiguration = BackendConfiguration()
    """Configuration for the backend"""

    frontend: FrontendConfiguration = FrontendConfiguration()
    """Configuration for the frontend"""

    debug: bool = True
    """Debug outputs"""

    secret: str = "development"
    """An arbitrary ascii string to sign sessions etc. Make sure to back it up!"""

    def __str__(self) -> str:
        return to_yaml_str(self, indent=2, exclude_none=False, add_comments=True)

    @classmethod
    def from_file(cls, path: Path) -> Self:
        """
        Loads the configuration from a file.

        Parameters
        ----------
        path : Path
            Path to the configuration YAML file.
        """
        if not path.is_file():
            raise ValueError(
                f"Configuration file {path} does not exist or is not a file."
            )
        return parse_yaml_raw_as(cls, path.read_text(encoding="utf-8"))


Configuration.model_rebuild()  # Ensure the model is rebuilt to reflect any changes
