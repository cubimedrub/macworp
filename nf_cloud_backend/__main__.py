from nf_cloud_backend import app, env, config, socketio
from nf_cloud_backend.utility.configuration import Environment

if __name__ == '__main__':
    print(f"Start NF-Cloud webinterface in {env.name} mode on {config['interface']}:{config['port']}")
    socketio.run(app, config['interface'], config['port'])
