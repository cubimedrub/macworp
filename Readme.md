# Nextflow Cloud
Nextflow Cloud or NF-Cloud is a web interface to run nextflow workflows in a cloud environment.   
NF-Cloud has three compontents

## Backend
A web API written in [Flask](https://flask.palletsprojects.com/en/2.0.x/) for managing/scheduling workflows.

## Frontend
A web interface written in [NuxtJS](https://nuxtjs.org/). This is basically a GUI for the web interface.

## Worker
A worker witten in python which runs the scheduled workflows.

![NF-Cloud structure](./nf-cloud.png)


## Developing
Make sure the dependencies in

* [Frontend](nf_cloud_frontend/Readme.md)
* [Backend](nf_cloud_backend/Readme.md)
* [Worker](nf_cloud_backend/Readme.md)

are installed.

### Prepare development environment
```bash
# Create environment
conda env create -f environment.yaml

# Already creates the environment and need updates???
conda env update -f environment.yml --prune

# Upgrade pip and setuptools
conda activate nf_cloud

# Install modules for development
pip install -r requirements.dev.txt

# Install node requirements
yarn --cwd ./nf_cloud_frontend install
```


### Start
```bash
# Shell 1
docker-compose up
# Shell 2
python -m nf_cloud_backend database migrate
python -m nf_cloud_backend utility rabbitmq prepare
honcho -e dev.env start
```

| Component | Access | User | Password |
| --- | --- | --- | --- |
| Frontend | `http://localhost:5001` | | |
| API | `http://localhost:3001` | | |
| Fusionauth | `http://localhost:9011` | `developer@example.com` | `developer` |

For development, Flask is configured to add CORS-Headers by default.

### Database migrations
To keep track of database changes, this project uses [`peewee_migrate`](https://github.com/klen/peewee_migrate).
Migrations are located in `nf_cloud_backend/migrations/`. For now this needs to be added manually when using `pw_migrate`. 

#### Create new migration
`pw_migrate create --directory nf_cloud_backend/migrations --database postgresql://postgres:developer@127.0.0.1:5434/nf_cloud "<description>"`

#### Run migrations
`pw_migrate migrate --database 'postgresql://postgres:developer@127.0.0.1:5434/nf_cloud' --directory nf_cloud_backend/migrations`

#### Accessing the database
`psql postgresql://postgres:developer@127.0.0.1:5434/nf_cloud`

### Testing deployment
In production NF-Cloud is designed to run as Gunicorn service behind a NginX reverse proxy. This setup can be tested with `make production-test`   
This will build and start back- & frontend containers, start multiple instances and put them behind a NginX reverse proxy, running on the port 16160 (NF-Cloud) and 16161 (Fusionauth). Both applications are served under HTTPS with a self signed certificate for testing and the hosts name as domain.   
A worker can be started with `make production-worker-test`

## Production

### External services
#### RabbitMQ
Set the `consumer_timeout` to a high value. Otherwise scheduled workflows can't be acknowledged after finishing and getting rescheduled

### Preparation
The following part contains CLI usage of `nf_cloud_backend`, which assume you use the nativ installation. If you run `nf_cloud_backend` in docker just replace `python -m nf_cloud_backend` with `docker run mpc/nf_cloud_backend`.

#### Docker image
##### Creation
Create the docker image with 
```
docker build mpc/nf-cloud-backend .
```
You can use the build arguments (`--build-arg`) `USER_ID` and `GROUP_ID` to change the user and group ID of the container user. Useful for harmonizing the ownership of the uploaded files with a local user.

##### Usage
The images entrypoint is the `nf_cloud_backend` command line interface, so users can start using command line arguments (e.g. `--help`). If the option `serve --gunicorn` is given, the image will start NF-Cloud as Gunicorn application, unlike the native installation which prints the Gunicorn parameters.    
NF-Cloud is running in the folder `/home/app`. So please mount a local configuration accordingly when running the images, e.g.: `docke run -v <some-local-config>:/home/app/nf_cloud.local.config.yaml`. To persist the uploaded files a persistent volume or bind-mount should be used. 

**Attention**: The worker needs to be deployed natively to use Docker containers in workflows.


#### Native
Clone the repository and use Conda or Miniconda to create an environment with all necessary dependencies: `conda env create -f environment.yml` (the environment name will be `nf_cloud`)

##### Usage
Make sure to activate the environment and go into the root folder of the repository before using `nf_cloud_backend` or use `conda run -n nf_cloud python -m nf_cloud_worker`


##### Worker
Run `conda run -n nf_cloud python`

#### Create a new configuration file
`python -m nf_cloud_backend utility config create .` this will create a new config named `nf_cloud.local.config.yaml` in the current directory. You can also print the configuration with `python -m nf_cloud_backend utility config print` (useful for piping the results from a Docker container).


## ToDos
* Try to move Nextflow intermediate result folders into a subfolder.
* Add some more inputs
    * `Radio` 
    * `Select`
* Add description to workflows
* Improve logging
    * Save Nextflow stdout/stderr
    * Enable report and show resulting HTML in web interface
* Rework docker deployment
    * Investigate if the worker is able to run Nextflow in Docker containers when running in container itself


