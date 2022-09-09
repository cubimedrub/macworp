# Nextflow workers
Simple python module, which fetches the scheduled nextflow workflows from RabbitMQ and executes them.

## Dependencies
* Python 3.x
* [pyenv](https://github.com/pyenv/pyenv)
* [pipenv](https://pipenv.pypa.io/en/latest/)
* [nextflow](https://www.nextflow.io/) - A [special version of Nextflow](https://github.com/di-hardt/nextflow/commit/3ef40cc139c81e535ac18b0ab7a6099c61af2591) is required which supports weblogs with basic authentication. A [feature request](https://github.com/nextflow-io/nextflow/issues/3198) is already issued to merge it into the main repo.

## Configuration
Configuration is done by a command line interface, see: `python -m nf_cloud_worker --help`
| Parameter | Description |
| --- | --- |
| --nf-cloud-url | Base-URL where the NF-Cloud instance is running: e.g. `http://localhost:3001` |
| --rabbitmq-url | URL for accessing the RabbitMQ-Server, e.g. `ampq://user:password@host:port/url-encoded-namespace` |
| --workflow-queue | Name of the RabbitMQ workflow queue |
| --workflow-data-path | Path to the directory where NF-Cloud uploads the workflow data |
| --workflows | Workflow configuration YAML-files (can be provided multiple times). Same as the [nf_cloud_backend configuration](../nf_cloud_backend/Readme.md#Configuration), only the `workflow` key is necessary. |
| --api-user | API user for the worker, set in in config.yaml |
| --api-password | API passwor for the worker, set in in config.yaml |


## Development
For development put the nextflow binary into the root folder of the project before starting the application via `pipenv run dev`.