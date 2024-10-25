# Nextflow workers
Simple python module, which fetches the scheduled nextflow workflows from RabbitMQ and executes them.

## Dependencies
* Python 3.x
* Conda--derivate (conda, micromamba, ...)
* Java (version depends on Nextflow)
* [Nextflow](https://www.nextflow.io/)

## Install


## Configuration
Configuration is done by a command line interface, see: `python -m macworp_worker --help`
| Parameter | Description |
| --- | --- |
| --macworp-url | Base-URL where the NF-Cloud instance is running: e.g. `http://localhost:3001` |
| --rabbitmq-url | URL for accessing the RabbitMQ-Server, e.g. `ampq://user:password@host:port/url-encoded-namespace` |
| --workflow-queue | Name of the RabbitMQ workflow queue |
| --workflow-data-path | Path to the directory where NF-Cloud uploads the workflow data |
| --workflows | Workflow configuration YAML-files (can be provided multiple times). Same as the [macworp configuration](../macworp/Readme.md#Configuration), only the `workflow` key is necessary. |
| --api-user | API user for the worker, set in in config.yaml |
| --api-password | API passwor for the worker, set in in config.yaml |


## Development
For development put the nextflow binary into the root folder of the project before starting the application via `pipenv run dev`.