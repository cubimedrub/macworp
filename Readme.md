# MAcWorP - Massive aCcessible Workflow Platform

MaCWorP is a web application to give workflow developers a simple way to make their workflows easily accessible via a web browser for everyone. 

* What it **does** for you
    * Graphical user interface for
        * Data up- & download
        * Parameter input
        * Rendered results (interactive plots, PDFs, images, tables, ...)
    * Authentication
        * Simple file based authentication (easy to set up, recommended for smaller teams, labs or institutes)
        * OpenIDConnect (e.g. connect to ELIXIR's Life Science Login)
    * Distributed execution of a workflow on different workers
    * Coming soon(ish):
        * Support for multiple workflow engines (currently Nextflow is working)
        * Fine grained access control to projects and workflows including sharing with others.
* What it **does not** for you
    * Scaling - This is still the job of the workflow engine. E.g. you can configure Nextflow to use K8s or Slurm when started by MAcWorP workers but setting these executors up is still a separate job for an admin.


## Production

### External services
#### RabbitMQ
Set the `consumer_timeout` to a high value. Otherwise scheduled workflows can't be acknowledged after finishing and getting rescheduled

### Preparation
The following part contains CLI usage of `macworp`, which assume you use the nativ installation. If you run `macworp` in docker just replace `python -m macworp` with `docker run mpc/macworp`.

#### Docker image
##### Creation (backend)
Create the docker image with 
```
docker build -t mpc/macworp-backend:latest -f docker/backend.dockerfile .
```
You can use the build arguments (`--build-arg`) `NEW_MAMBA_USER_ID` and `NEW_MAMBA_USER_GID` to change the user and group ID of the container user. Useful for harmonizing the ownership of the uploaded files with a local user.

##### Creation (frontend)
```
docker build -t mpc/macworp-frontend:latest -f docker/frontend.dockerfile .
```

##### Usage
The images entrypoint is the `macworp` command line interface, so users can start using command line arguments (e.g. `--help`). If the option `serve --gunicorn` is given, the image will start NF-Cloud as Gunicorn application, unlike the native installation which prints the Gunicorn parameters.    
NF-Cloud is running in the folder `/home/app`. So please mount a local configuration accordingly when running the images, e.g.: `docke run -v <some-local-config>:/home/app/macworp.local.config.yaml`. To persist the uploaded files a persistent volume or bind-mount should be used. 

**Attention**: The worker needs to be deployed natively to use Docker containers in workflows.


#### Native
Clone the repository and use Conda or Miniconda to create an environment with all necessary dependencies: `conda env create -f environment.yml` (the environment name will be `macworp`)

##### Usage
Make sure to activate the environment and go into the root folder of the repository before using `macworp` or use `conda run -n macworp python -m macworp_worker`


##### Worker
Run `conda run -n macworp python`

#### Create a new configuration file
`python -m macworp utility config create .` this will create a new config named `macworp.local.config.yaml` in the current directory. You can also print the configuration with `python -m macworp utility config print` (useful for piping the results from a Docker container).


## Workflow developer

### Best practice
1. In general it is a good idea to use publicly available Docker container to manage your dependencies as most workflow engines can download containers by themselves. For help see:
    * [Nextflow](https://snakemake.readthedocs.io/en/stable/snakefiles/deployment.html#running-jobs-in-containers)
    * [Snakemake](https://snakemake.readthedocs.io/en/stable/snakefiles/deployment.html#running-jobs-in-containers)
    However it is possible use dependencies installed via Conda or package managers, but it has to be done manually on each worker. 

### Input elements
List is coming soon.
