import bjoern

from nf_cloud_backend import app, env, config
from nf_cloud_backend.utility.configuration import Environment

if __name__ == '__main__':
    print(f"Start NF-Cloud webinterface in {env.name} mode on {config['interface']}:{config['port']}")

    if env == Environment.development:
        app.run(config['interface'], config['port'])
    elif env == Environment.production:
        bjoern.run(app, config['interface'], config['port'])
