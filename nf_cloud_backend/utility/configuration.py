import os
import argparse
import pathlib
import re
from enum import Enum, auto
from yaml import load as yaml_load, Loader as YamlLoader

class Environment(Enum):
    production = auto()
    development = auto()

    @classmethod
    def get(cls, environment: str):
        try:
            return cls[environment.lower()]
        except KeyError:
            raise KeyError(f"Argument 'environment' has to be '{cls.production.name}' or '{cls.development.name}'.")

class Configuration():
    @staticmethod
    def get_config_and_env(consider_cli: bool = True) -> tuple:
        environment = Environment.get(os.getenv('NF_CLOUD_WEB_ENV', Environment.development.name))

        cli_args = None
        if consider_cli:
            cli_args = Configuration._parse_cli()
            # Override with CLI argument
            if cli_args.environment:
                environment = Environment.get(cli_args.environment)

        # Read config from files
        config = Configuration._read_files(environment)
        
        if consider_cli:
            # Overwrite with CLI argument
            if cli_args.interface:
                config['interface'] = cli_args.interface

            # Overwrite with CLI argument
            if cli_args.port:
                config['port'] = cli_args.port

        Configuration._validate_config(config)

        return config, environment

    @staticmethod
    def _read_files(environment: Environment) -> dict:
        current_work_dir = pathlib.Path.cwd()
        config = {}
        default_config_file = current_work_dir.joinpath('config.yaml')
        with default_config_file.open('r') as config_file:
            new_config = yaml_load(config_file.read(), Loader=YamlLoader)
            if new_config:
                config = Configuration._merge_dicts_recursively(new_config, config)

        if environment == Environment.development:
            development_config_file = current_work_dir.joinpath('config.development.yaml')
            with development_config_file.open('r') as config_file:
                new_config = yaml_load(config_file.read(), Loader=YamlLoader)
                if new_config:
                    config = Configuration._merge_dicts_recursively(new_config, config)

        if environment == Environment.production:
            production_config_file = current_work_dir.joinpath('config.production.yaml')
            with production_config_file.open('r') as config_file:
                new_config = yaml_load(config_file.read(), Loader=YamlLoader)
                if new_config:
                    config = Configuration._merge_dicts_recursively(new_config, config)

        local_config_file = current_work_dir.joinpath('config.local.yaml')
        if local_config_file.is_file():
            with local_config_file.open('r') as config_file:
                new_config = yaml_load(config_file.read(), Loader=YamlLoader)
                if new_config:
                    config = Configuration._merge_dicts_recursively(new_config, config)

        return config

    @staticmethod
    def _parse_cli():
        cli_parser = argparse.ArgumentParser(description='Run database webinterface')
        cli_parser.add_argument('--environment', '-e', required=False, choices=[Environment.production.name, Environment.development.name], help='Sets the execution environment (Overrides environment variable MDCHQ_ENV).')
        cli_parser.add_argument('--interface', '-i', type=str, required=False, help='Sets on which interface the HQ is running.')
        cli_parser.add_argument('--port', '-p', type=int, required=False, help='Sets on which port the HQ is running.')

        return cli_parser.parse_args()

    @classmethod
    def _validate_config(cls, config: dict) -> bool:
        try:
            cls._validate_type(config['debug'], bool, 'boolean', 'debug')
            cls._validate_type(config['interface'], str, 'ip string', 'interface')
            cls._validate_type(config['port'], int, 'integer', 'port')
            cls._validate_type(config['use_https'], bool, 'boolean', 'use_https')
            cls._validate_type(config['upload_path'], str, 'string', 'upload_path')
            cls._validate_ascii_string(config['secret'], 'secret')
            cls._validate_psql_url(config['database']['url'], 'database.url')
            cls._validate_type(config['database']['pool_size'], int, 'integer', 'database.pool_size')
            cls._validate_amqp_url(config['rabbit_mq']['url'], 'rabbit_mq.url')
            cls._validate_type(config['workflows'], dict, 'dict', 'workflows.local')
        except KeyError as key_error:
            raise KeyError(f"The configuration key {key_error} is missing.")

    @staticmethod
    def _validate_psql_url(url, key_path: str):
        Configuration._validate_type(url, str, 'string', key_path)
        if not url.startswith('postgresql://'):
            raise TypeError(f"{key_path} must start with 'postgresql://'.")
        return True
    
    @staticmethod
    def _validate_amqp_url(url, key_path: str):
        Configuration._validate_type(url, str, 'string', key_path)
        if not url.startswith('amqp://'):
            raise TypeError(f"{key_path} must start with 'amqp://'.")
        return True

    @staticmethod
    def _validate_type(value, expected_type, expected_type_as_str: str, key_path: str):
        if not isinstance(value, expected_type):
            raise TypeError(f"Configuration key '{key_path}' is not of type {expected_type_as_str}.")
        return True

    @staticmethod
    def _validate_ascii_string(string, key_path: str):
        Configuration._validate_type(string, str, 'string', key_path)
        if not all(ord(char) < 128 for char in string):
            raise TypeError(f"Configuration key '{key_path}' contains non ascii character.")
        return True

    @staticmethod
    def _merge_dicts_recursively(source, destination):
        for key, value in source.items():
            if isinstance(value, dict):
                node = destination.setdefault(key, {})
                Configuration._merge_dicts_recursively(value, node)
            else:
                destination[key] = value

        return destination

