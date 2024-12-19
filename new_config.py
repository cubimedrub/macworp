class Database(BaseModel):
    """Database configuration"""

    url: str = Field(default="postgresql://postgres:developer@127.0.0.1:5434/macworp")
    """
    Database url. Format `postgresql://<optional_user>:<optional_password>@<host>:<port>/<database>`
    , default: postgresql://postgres:developer@127.0.0.1:5434/macworp
    """

    pool_size: int = Field(default=1)
    """Number of connections. Currently not used, default: 1"""


class Matomo(BaseModel):
    """Settings to for Matomo tracking"""

    enabled: bool = Field(default=False)
    """Enable tracking, default: false"""

    url: Optional[str] = Field(default=None)
    """Matomo URL"""

    site_id: Optional[int] = Field(default=None)
    """Matomo site ID"""

    auth_token: Optional[str] = Field(default=None)
    """Matomo auth token"""

    @model_validator(mode="after")
    def prevent_none_when_enabled(self) -> Self:
        """
        Makes sure that mandatory values are given when Matomo is enabled
        """
        if self.enabled:
            if self.url is None:
                raise ValueError("Matomo is enabled, but no Matomo URL is not given.")
            if self.site_id is None:
                raise ValueError(
                    "Matomo is enabled, but no Matomo site ID is not given."
                )
            if self.auth_token is None:
                raise ValueError(
                    "Matomo is enabled, but no Matomo auth token is not given."
                )
        return self


class RabbitMqSettings(BaseModel):
    """Settings for RabbitMQ"""

    url: str = Field(default="amqp://admin:developer@127.0.0.1:5674/%2f")
    """
    RabbitMQ url. Make sure to encode the virtual host:
    https://pika.readthedocs.io/en/stable/modules/parameters.html#pika.connection.URLParameters
    """

    project_workflow_queue: str = Field(default="project_workflow")
    """Queue for scheduled projects"""


class WorkerCredentials(BaseModel):
    """Credentials for the worker"""

    username: str = Field(default="worker")
    """Username, default: worker"""

    password: str = Field(default="developer")
    """Password, default: developer"""


class FileLoginPriovider(BaseModel):
    """Login provider for file based authentication"""

    name: str
    """Name of the provider"""

    description: str
    """Description of the provider"""

    file: str
    """Path to the file"""

    expires_in: int
    """Expiration time in seconds"""


class OpenIdLoginProvider(BaseModel):
    """Login provider for OpenID authentication"""

    name: str
    """Name of the provider"""

    description: str
    """Description of the provider"""

    client_id: str
    """Client ID"""

    client_secret: str
    """Client secret"""

    discovery_url: str
    """Discovery URL"""

    scope: Optional[str]
    """Scope"""

    verify_ssl: bool
    """Verify SSL"""


class Confidantic(BaseModel):
    """Configuration for the application"""

    __instance: ClassVar[Optional[Self]] = None
    """Singleton instance"""

    LOCAL_CONFIG_NAME: ClassVar[str] = "macworp.local.config.yaml"
    """ Name for local configuration
    """

    YAML_ENV_VAR_REG: ClassVar[re.Pattern] = re.compile(r"ENV\['(?P<env_name>.+?)'\]")
    """ Regex for matching environment variables
    """

    KEY_PATHS_TO_OVERRIDE: ClassVar[Tuple[str, ...]] = ("login_providers",)
    """Key path to override when merging dictionaries"""

    interface: str = Field(default="0.0.0.0")
    """Interface to use, default: 0.0.0.0 (all interfaces)"""

    port: int = Field(default=3001)
    """Port to use"""

    use_reverse_proxy: bool = Field(default=False)
    """
    Set to true if you use a reverse proxy to enable the addition of
    `x-(for|proto|host|prefix)` headers, default: false
    """

    secret: str = Field(default="development")
    """
    An arbitrary ASCII string to sign sessions etc. Make sure to back it up!, 
    default: development
    """

    debug: bool = Field(default=True)
    """Enable debug behavior like debug level during logging, default: true"""

    upload_path: str = Field(default="./uploads")
    """Project directory for uploaded files, default: ./uploads"""

    frontend_host_url: Optional[str] = Field(default="http://localhost:5001")
    """
    Redirect after successful login. 
    Can be null (~) when frontend and backend using the same domain and port
    E.g. https://macworp.mpc.rub.de, default: http://localhost:5001
    """

    redis_url: str = Field(default="redis://localhost:6380/0")
    """Redis URL. Format: redis://<host>:<port>/<db>, default: redis://localhost:6380/0"""

    database: Database = Database()
    """Database configuration"""

    matomo: Matomo = Matomo()
    """Matomo configuration"""

    worker_crendentials: WorkerCredentials = WorkerCredentials()
    """Worker credentials"""

    login_providers: Dict[str, Union[OpenIdLoginProvider, FileLoginPriovider]] = Field(
        default={
            "openid": {
                "dev": OpenIdLoginProvider(
                    name="openid",
                    description="Login with local Fusionauth",
                    client_id="c63f35e8-66dc-4ec3-bb34-b29803ba72f1",
                    client_secret="8CLrA9gAVmlM7GKKCnMnT9MHNvUMqXNVWog2kWUuC7Y2iBOnFIh9X0rpCqfMcpfQkEZfCBfxMqfz5FYC6cduB20kxqV5Ysq",  # pylint: disable=line-too-long
                    discovery_url="http://localhost:9011/e8a19b47-52ca-35c0-b34b-cc4ab0b4fa84/.well-known/openid-configuration",  # pylint: disable=line-too-long
                    scope="offline_access",
                    verify_ssl=False,
                )
            },
            "file": {
                "dev": FileLoginPriovider(
                    name="file",
                    description="Login with local file",
                    file="./dev_auth.yaml",
                    expires_in=4294967296,
                )
            },
        }
    )
    """Login providers"""

    @classmethod
    def __new__(cls, *args, **kwargs) -> Self:
        """Returns the singleton instance if not none or loads a new instance from yaml"""

        if cls.__instance is not None:
            return cls.__instance
        return cls.form_yaml()

    @classmethod
    def env_resolver(cls, config: str) -> str:
        """
        Resolves environment variables in given YAML config.
        Environment variables can be used like:
        ```
        text: i'm a yaml config
        used_environment: ENV['name-of-the-env-var']
        ```

        Parameters
        ----------
        config : str
            YAML config

        Returns
        -------
        str
            YAML config with resolved environment variables.
        """

        def resolve(match: re.Match) -> Optional[str]:
            return os.getenv(match.group("env_name"))

        return re.sub(cls.YAML_ENV_VAR_REG, resolve, config)

    @classmethod
    def merge_dicts_recursively(
        cls,
        source: Dict[str, Any],
        destination: Dict[str, Any],
        key_path: List[str] = [],
    ) -> Dict[str, Any]:
        """
        Merges source dictionary into destination dictionary recursively
        but overrides every key path in KEY_PATHS_TO_OVERRIDE completely.

        Parameters
        ----------
        source : Dict[str, Any]
            Source dictionary
        destination : Dict[str, Any]
            Destination dictionary
        key_path : List[str]
            Path of keys to detect key paths to override.

        Returns
        -------
        Dict[str, Any]
            Merged dictionary
        """
        for key, value in source.items():
            new_key_path: List[str] = deepcopy(key_path)
            new_key_path.append(key)
            if ".".join(key_path) not in cls.KEY_PATHS_TO_OVERRIDE and isinstance(
                value, dict
            ):
                node = destination.setdefault(key, {})
                cls.merge_dicts_recursively(value, node, new_key_path)
            else:
                destination[key] = value
        return destination

    @classmethod
    def form_yaml(cls):
        """
        Initializes configuration
        """
        default_config = cls()
        default_config_dict = default_config.model_dump()

        config_path = Path(f"./{cls.LOCAL_CONFIG_NAME}")

        if config_path.is_file():
            local_config = config_path.read_text(encoding="utf-8")
            local_config = cls.env_resolver(local_config)
            local_config_dict = yaml_load(local_config, Loader=YamlLoader)
            config = cls.merge_dicts_recursively(local_config_dict, default_config_dict)
            return cls.model_validate(config)

        return default_config
