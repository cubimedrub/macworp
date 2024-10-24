# MaCWorP - Massive aCcessible Workflow Platform

MaCWorp is a web application to give workflow developers a simple way to make their workflows easily accessible via a web browser for everyone. 

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
    * Scaling - This is still the job of the workflow engine. E.g. you can configure Nextflow to use K8s or Slurm when started by MaCWorP workers but setting these executors up is still a separate job for an admin.


## Backend
A web API written in [Flask](https://flask.palletsprojects.com/en/2.0.x/) for managing/scheduling workflows.

## Frontend
A web interface written in [NuxtJS](https://nuxtjs.org/). This is basically a GUI for the web API.

## Worker
A worker written in python which runs the scheduled workflows.

![MaCWorP structure](./macworp.png)

## Developing
Make sure the dependencies in

* [Frontend](frontend/Readme.md)
* [Backend](backend/Readme.md)
* [Worker](worker/Readme.md)

are installed.

### Prepare development environment
```bash
# Create environment
conda env create -f environment.yml

# Already creates the environment and need updates???
conda env update -f environment.yml --prune

# Upgrade pip and setuptools
conda activate nf_cloud

# Install node requirements
yarn --cwd ./frontend install
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

### Test data
Use `python -m nf_cloud_backend database seed --drop` to insert some test records (e.g. workflows) into the database.

## Production

### External services
#### RabbitMQ
Set the `consumer_timeout` to a high value. Otherwise scheduled workflows can't be acknowledged after finishing and getting rescheduled

### Preparation
The following part contains CLI usage of `nf_cloud_backend`, which assume you use the nativ installation. If you run `nf_cloud_backend` in docker just replace `python -m nf_cloud_backend` with `docker run mpc/nf_cloud_backend`.

#### Docker image
##### Creation (backend)
Create the docker image with 
```
docker build -t mpc/nf-cloud-backend:latest -f docker/backend.dockerfile .
```
You can use the build arguments (`--build-arg`) `NEW_MAMBA_USER_ID` and `NEW_MAMBA_USER_GID` to change the user and group ID of the container user. Useful for harmonizing the ownership of the uploaded files with a local user.

##### Creation (frontend)
```
docker build -t mpc/nf-cloud-frontend:latest -f docker/frontend.dockerfile .
```

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


## Workflow developer

### Input elements
List is coming soon.

### Results
MAcWorP is able to render a couple of file formats directly in the browser. The file extension tells the renderer how the element should be rendered.   
Below is a list of supported elements:

| Type | File extension | Description |
| --- | --- | --- |
| Raster graphics | `.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`, `.bmp` | Images are display using [Viewer.js](https://fengyuanchen.github.io/viewerjs/). This gives the user some useful features, like zooming |
| Vector graphics | `.svg`, `.images.svg` | SVG files can be rendered iun two ways, embedded into the DOM (`.svg`) which automatically scales it, or within an img-Tag (`.image.svg`) |
| PDF | `.pdf` | PDFs are shown using the browser integrated PDF viewer |
| Table | `.csv`, `.tsv`, `.xlsx` | Tables are rendered using the HTML-table elements. As two many cells will make browser unresponsive, tables with to many elements will be paginated |
| Interactive plots | `.plolty.json` | To generate Plolty conform JSON files, have a look into the [documentation](https://plotly.com/python-api-reference/generated/plotly.io.to_json.html#plotly.io.to_json). These kind of plots have few advantages for the user, like the ability to enable/disable traces and zoom. MAcWorP is also adding a JSON editor to change the plot layout or colors. |


#### Metadata files
Each result file can be annotated with a header and description, by adding a JSON formatted MAcWorP metadata file next to the result file. It should have the same name as the result file plus the extension `.mmdata`, e.g.:
* File to annotate file: `./some_barplot.png`
* Metadata file:  `./some_barplot.png.mmdata`

The content would look like this:
```json
{
    "header": "That some kind of a bar plot",
    "description": "Some very lengthy description, telling users what the barplot is showing."
}
```
Header and description are then rendered in the frontend. If no metadata is given, the header is replaced by the filename and the description skipped.


Have a look into the [results demo workflow](./demo_workflows/result_demo/)



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


