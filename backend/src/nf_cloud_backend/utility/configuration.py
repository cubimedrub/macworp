# Module for configuration

# std imports
from __future__ import annotations
from copy import deepcopy
from io import StringIO
import os
from pathlib import Path
import re
from typing import ClassVar, Dict, Any, List, Tuple, Type

# 3rd party imports
from yaml import load as yaml_load, Loader as YamlLoader

class Configuration():
    """
    Application configuration
    """

    LOCAL_CONFIG_NAME: ClassVar[str] = "nf_cloud.local.config.yaml"
    """ Name for local configuration
    """

    YAML_ENV_VAR_REG: ClassVar[re.Pattern] = re.compile(r"ENV\['(?P<env_name>.+?)'\]")
    """ Regex for matching environment variables
    """

    KEY_PATHS_TO_OVERRIDE: ClassVar[Tuple[str, ...]] = (
        "login_providers",
    )

    DEFAULT_CONFIG: ClassVar[str] = """
# Interface to use (0.0.0.0 = all interfaces)
interface: 0.0.0.0
# Port to use
port: 3001
# Set to true if you use a reverse proxy
use_reverse_proxy: false
# An arbitrary ascii string to sign sessions etc. Make sure to back it up!
secret: "development"
# Debug outputs
debug: true
# Path to put working directories
upload_path: "./uploads"
# Set this in production to the base url of NF Cloud. E.g. https://nf-cloud.mpc.rub.de
# frontend_host_url
# Database
database:
  # Database url: 
  url: postgresql://postgres:developer@127.0.0.1:5434/nf_cloud
  # Number of connections
  pool_size: 1
matomo:
  enabled: false
  url: ""
  site_id: 1
  auth_token: ""
rabbit_mq:
  # RabbitMQ url. Make sure to encode the virtual host: https://pika.readthedocs.io/en/stable/modules/parameters.html#pika.connection.URLParameters
  url: "amqp://admin:developer@127.0.0.1:5674/%2f"
  # Queue for scheduled projects
  project_workflow_queue: project_workflow
redis_url: redis://localhost:6380/0
# Basic auth for worker
worker_credentials:
  username: "worker"
  password: "developer"
login_providers:    # Will be completely overridden by local config
  openid:
    dev:
        description: Login with local Fusionauth
        client_id: c63f35e8-66dc-4ec3-bb34-b29803ba72f1
        client_secret: 8CLrA9gAVmlM7GKKCnMnT9MHNvUMqXNVWog2kWUuC7Y2iBOnFIh9X0rpCqfMcpfQkEZfCBfxMqfz5FYC6cduB20kxqV5Ysq
        discovery_url: http://localhost:9011/9afb0e9a-21c1-a9f5-932e-157de0ef3684/.well-known/openid-configuration
        scope: "offline_access"
        verify_ssl: false
    # E.g.
    # google:
    #   description: Login with Google
    #   client_id: xyz
    #   client_secret: abc
    #   client_discovery_url: https://...
    #   # Optional, may be given by provider
    #   scope: ~
  file:
    dev:
      description: Login with local file
      file: ./dev_auth.yaml
      expires_in: 4294967296
# Redirect after successful login. Can be null (~) when frontend and backend using the same domain and port
frontend_host_url: http://localhost:5001
"""
    """Default configuration.
    """

    __values: ClassVar[Dict[str, Any]] = {}

    @staticmethod
    def values() -> Dict[str, Any]:
        return Configuration.__values

    @classmethod
    def initialize(cls):
        """
        Initializes configuration
        """
        config = yaml_load(cls.DEFAULT_CONFIG, Loader=YamlLoader)
        config_path: Path = Path(f"./{cls.LOCAL_CONFIG_NAME}")
        if config_path.is_file():
            with config_path.open("r", encoding="utf-8") as config_file:
                local_config_io: StringIO = StringIO(cls.env_resolver(config_file.read()))
                new_config = yaml_load(local_config_io, Loader=YamlLoader)
                config = cls._merge_dicts_recursively(new_config, config)
        cls._validate_config(config)

        cls.__values = config

    @classmethod
    def _validate_config(cls, config: Dict[str, Any]):
        """
        Checks config values

        Parameters
        ----------
        config : Dict[str, Any]
            Config

        Raises
        ------
        KeyError
            If key path not found
        """
        try:
            cls._validate_type(config['debug'], bool, 'boolean', 'debug')
            cls._validate_type(config['interface'], str, 'ip string', 'interface')
            cls._validate_type(config['port'], int, 'integer', 'port')
            cls._validate_type(config['use_reverse_proxy'], bool, 'boolean', 'use_reverse_proxy')
            cls._validate_ascii_string(config['secret'], 'secret')
            cls._validate_psql_url(config['database']['url'], 'database.url')
            cls._validate_type(config['database']['pool_size'], int, 'integer', 'database.pool_size')
        except KeyError as key_error:
            raise KeyError(f"The configuration key {key_error} is missing.") from key_error

    @staticmethod
    def _validate_psql_url(url: Any, key_path: str) -> bool:
        """
        Checks if url is Postgresql URL

        Parameters
        ----------
        url : Any
            Candidate to check
        key_path : str
            Path if keys to value

        Returns
        -------
        bool

        Raises
        ------
        TypeError
            If not a postgres url
        """
        Configuration._validate_type(url, str, 'string', key_path)
        if not url.startswith('postgresql://'):
            raise TypeError(f"{key_path} must start with 'postgresql://'.")
        return True

    @staticmethod
    def _validate_type(value: Any, expected_type: Type, expected_type_as_str: str, key_path: str) -> bool:
        """
        Checks if the given string is an ASCII string

        Parameters
        ----------
        value : Any
            Candidate to check
        expected_type : Type
            Expected type
        expected_type_as_str : str
            Expected type as string
        key_path : str
            Path if keys to value

        Returns
        -------
        bool

        Raises
        ------
        TypeError
            If value is of expected type
        """
        if not isinstance(value, expected_type):
            raise TypeError(f"Configuration key '{key_path}' is not of type {expected_type_as_str}.")
        return True

    @staticmethod
    def _validate_ascii_string(value: Any, key_path: str) -> bool:
        """
        Checks if the given value is an ASCII string

        Parameters
        ----------
        value : Any
            Candidate to check
        key_path : str
            Path if keys to value

        Returns
        -------
        bool

        Raises
        ------
        TypeError
            If value is not an ASCII string
        """
        Configuration._validate_type(value, str, 'string', key_path)
        if not all(ord(char) < 128 for char in value):
            raise TypeError(f"Configuration key '{key_path}' contains non ascii character.")
        return True

    @staticmethod
    def _merge_dicts_recursively(source: Dict[str, Any], destination: Dict[str, Any], key_path: List[str] = []) -> Dict[str, Any]:
        """
        Merges source dictionary into destination dictionary.

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
            if ".".join(key_path) not in Configuration.KEY_PATHS_TO_OVERRIDE and isinstance(value, dict):
                node = destination.setdefault(key, {})
                Configuration._merge_dicts_recursively(value, node, new_key_path)
            else:
                destination[key] = value
        return destination

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
        def resolve(match: re.Match) -> str:
            return os.getenv(match.group("env_name"))

        return re.sub(cls.YAML_ENV_VAR_REG, resolve, config)


    @classmethod
    def create_default_config(cls, folder_path: Path):
        """
        Writes default config to file

        Parameters
        ----------
        config_path : Path
            Path to config file
        """
        config_path: Path = folder_path.joinpath(cls.LOCAL_CONFIG_NAME)
        if not config_path.is_file():
            with config_path.open("w") as config_file:
                config_file.write(cls.DEFAULT_CONFIG.strip())
        else:
            raise ValueError(f"path already contains a file named {cls.LOCAL_CONFIG_NAME}")

    @classmethod
    def print_default_config(cls):
        """
        Prints default config
        """
        print(cls.DEFAULT_CONFIG.strip())

    @staticmethod
    def create_config_file_by_cli(cli_args):
        """
        Write

        Parameters
        ----------
        cli_args : Any
            CLI arguments
        """
        Configuration.create_default_config(
            Path(cli_args.path)
        )

    @classmethod
    def print_default_config_by_cli(cls, _cli_args):
        """
        Prints default config
        """
        cls.print_default_config()

    @staticmethod
    def print_config_param(key_path: str):
      keys: List[str] = key_path.split(".")
      value: Any = Configuration.values()
      for key in keys:
        value = value[key]
      print(value)
