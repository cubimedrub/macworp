# Worker
MAcWorP's worker component fetches the scheduled workflows and executes the using the given workflow engine.

## Dependencies
* Conda-derivate (conda, micromamba, ...)
* Java (version depends on Nextflow)
* [Nextflow](https://www.nextflow.io/)
    * Version 24.09.2-edge until https://github.com/nextflow-io/nextflow/issues/5443#issuecomment-2445609593 us fixed

## Install
1. Clone the repository
2. `micromamba env create -f environment.yaml`
3. `micromamba activte macworp`
4. `python -m macworp_worker ...`


## Configuration
Configuration is done by a command line interface, see: `python -m macworp_worker --help`

| Parameter | Description |
| --- | --- |
| --nf-bin | Path to Nextflow binary |
| --sm-bin | Path Snakemake binary. As Snakemake is installed using the Conda, it's path can be found with `$(which snakemake)` |
| --macworp-url | Base-URL where the NF-Cloud instance is running: e.g. `http://localhost:3001` |
| --rabbitmq-url | URL for accessing the RabbitMQ-Server, e.g. `ampq://user:password@host:port/url-encoded-namespace` |
| --project-queue-name | Name of the RabbitMQ workflow queue |
| --projects-data-path | Path to the directory where user data is stored |
| --number-of-workers |  Number of concurrent workers. |
| --api-user | API user for the worker, set in in config.yaml |
| --api-password | API password for the worker, set in in config.yaml |
| --verbose, -v | Verbosity. Can be used more than once. Every usage will increase thr log level. |
| --keep-intermediate-files | Keeps intermediate file if set. Otherwise temporary workflow folder will be deleted after workflow execution is finished |
| --skip-cert-verification | Skips certificate verification when talking to the API. |
