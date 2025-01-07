"""Test the configuration module."""

import os
import unittest

from macworp_backend.utility.configuration import Configuration


class ConfigurationTest(unittest.TestCase):
    """Test the configuration module."""

    def test_configuration_merge(self):
        """Check correct merging of default and override configuration."""

        # pylint: disable=invalid-name
        EXPECTED_LOGIN_PROVIDERS = {
            "testoverride": {
                "dev": {
                    "description": "Override for testing",
                    "client_id": "c63f35e8-66dc-4ec3-bb34-b29803ba72f1",
                    "client_secret": "8CLrA9gAVmlM7GKKCnMnT9MHNvUMqXNVWog2kWUuC7Y2iBOnFIh9X0rpCqfMcpfQkEZfCBfxMqfz5FYC6cduB20kxqV5Ysq",
                    "discovery_url": "http://localhost:9011/e8a19b47-52ca-35c0-b34b-cc4ab0b4fa84/.well-known/openid-configuration",
                    "scope": "offline_access",
                    "verify_ssl": False,
                }
            }
        }

        # Set the override environment variable
        os.environ[Configuration.CONFIG_ENV_NAME] = (
            "backend/test_files/local.config.yml"
        )
        Configuration.initialize()

        # Check non-dict overrides
        self.assertEqual(Configuration.values()["interface"], "127.0.0.1")
        self.assertEqual(Configuration.values()["debug"], False)

        # Check recursive dict overrides
        self.assertEqual(Configuration.values()["matomo"]["auth_token"], "foobar")
        self.assertEqual(
            Configuration.values()["worker_credentials"]["password"], "foobar"
        )

        # Check listed overrides
        self.assertEqual(
            Configuration.values()["login_providers"], EXPECTED_LOGIN_PROVIDERS
        )

        # Check skipped values
        self.assertEqual(Configuration.values()["port"], 3001)
        self.assertEqual(Configuration.values()["use_reverse_proxy"], False)
        self.assertEqual(Configuration.values()["secret"], "development")
