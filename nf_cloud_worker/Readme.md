# Nextflow workers
Simple python module, which fetches the scheduled nextflow workflows from RabbitMQ and executes them.

## Dependencies
* Python 3.x
* [pyenv](https://github.com/pyenv/pyenv)
* [pipenv](https://pipenv.pypa.io/en/latest/)


## Configuration
Configuration is done by a command line interface, see: `python -m nf_cloud_worker --help`
| Parameter | Description |
| --- | --- |
| --nf-cloud-url | Base-URL where the NF-Cloud instance is running: e.g. `http://localhost:3001` |
| --rabbitmq-url | URL for accessing the RabbitMQ-Server, e.g. `ampq://user:password@host:port/url-encoded-namespace` |
| --workflow-queue | Name of the RabbitMQ workflow queue |
| --workflow-data-path | Path to the directory where NF-Cloud uploads the workflow data |
| --workflows | Workflow configuration YAML-files (can be provided multiple times). Same as the [nf_cloud_backend configuration](../nf_cloud_backend/Readme.md#Configuration), only the `workflow` key is necessary. |