# MAcWorP - Massive aCcessible Workflow Platform

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
    * Scaling - This is still the job of the workflow engine. E.g. you can configure Nextflow to use K8s or Slurm when started by MAcWorP workers but setting these executors up is still a separate job for an admin.

## Quickstart
You are interested and want to try MAcWroP?
1. A Unix-like operating system is mandatory: You are good to go with any popular Linux distribution, Windows Subsystem for Linux or macOS
2. Install [Docker](https://docs.docker.com/get-started/get-docker/)
    * On Linux make sure your user is allowed to access the Docker service by adding the user to the Docker group `usermod -aG docker <your-user>`
3. Install `make`
4. Install `git`
5. Clone the repository
6. `make production-test-up`

This will start the web interface on `https://<local-computer-name>:16160`, the complete URL is written to the file `PRODUCTION_TEST_URL`. It might change depending on your location (home, office, ...) as it includes the computers FQDN.


## Developing
### Structure

#### Backend
A web API written in [Flask](https://flask.palletsprojects.com/en/2.0.x/) for managing/scheduling workflows.

#### Frontend
A web interface written in [NuxtJS](https://nuxtjs.org/). This is basically a GUI for the web API.

#### Worker
A worker written in python which runs the scheduled workflows.

![MAcWorP structure](./macworp.png)

### Requirements
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
conda activate macworp

# Install node requirements
yarn --cwd ./frontend install
```


### Start
```bash
# Shell 1
docker-compose up
# Shell 2
python -m macworp database migrate
python -m macworp utility rabbitmq prepare
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
Migrations are located in `macworp/migrations/`. For now this needs to be added manually when using `pw_migrate`. 

#### Create new migration
`pw_migrate create --directory macworp/migrations --database postgresql://postgres:developer@127.0.0.1:5434/macworp "<description>"`

#### Run migrations
`pw_migrate migrate --database 'postgresql://postgres:developer@127.0.0.1:5434/macworp' --directory macworp/migrations`

#### Accessing the database
`psql postgresql://postgres:developer@127.0.0.1:5434/macworp`

### Testing deployment
In production NF-Cloud is designed to run as Gunicorn service behind a NginX reverse proxy. This setup can be tested with `make production-test`   
This will build and start back- & frontend containers, start multiple instances and put them behind a NginX reverse proxy, running on the port 16160 (NF-Cloud) and 16161 (Fusionauth). Both applications are served under HTTPS with a self signed certificate for testing and the hosts name as domain.   
A worker can be started with `make production-worker-test`

### Test data
Use `python -m macworp database seed --drop` to insert some test records (e.g. workflows) into the database.

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
| Plain text | `.txt` | Show the content of the text file |


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
