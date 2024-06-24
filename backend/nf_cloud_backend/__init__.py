from oauthlib.oauth2 import WebApplicationClient
from nf_cloud_backend.configuration import Configuration

Configuration.initialize()

openid_clients = {
    provider: WebApplicationClient(provider_data["client_id"])
    for provider, provider_data in Configuration.values()["login_providers"]["openid"].items()
}