# Nextflow Cloud
Nextflow Cloud is a web interface to run nextflow workflows in a cloud environment.

## Dependencies
* Build tools (Ubuntu: `build-essential`, Arch Linux: `base-devel`)
* C/C++-header for PostgreSQL (Ubuntu: `libpq-dev`, Arch Linux: `postgresql-libs`)
* C/C++-headers for libev (Ubuntu: `libev-dev`, Arch Linux: `libev`)
* Rust Compiler
* Docker
* Python 3.x
* [pyenv](https://github.com/pyenv/pyenv)
* [pipenv](https://pipenv.pypa.io/en/latest/)
* NodeJS 14.x
* yarn

### For development only
* GIT
* Docker Compose

## Development
Nextflow Cloud Web consists of two parts.
1. `nf_cloud_backend` - A [Flask](https://flask.palletsprojects.com/en/2.0.x/) web application, providing the API endpoints.
2. `nf_cloud_frontend` - A [NuxtJS](https://nuxtjs.org/) application, providing the web pages.
### Prepare development environment
```bash
# Install the correct python version
pyenv install $(cat .python-version)

# Create an environment
pipenv install -d

# Install node requirements
yarn --cwd ./nf_cloud_frontend install


```
Create a `config.local.yaml` (see [Configuration](#backend-configuration)) and set a local Nextflow Cloud for development.

### Start the developing
```bash
# Start the database
docker-compose up
# Migrate database changes
alembic upgrade head
# Start the app
pipenv run dev
```
The frontend can than be accessed on port `http://localhost:5000` and the API on `localhost:3001`.   
For development, Flask is configured to add CORS-Headers by default.

## Configuration
### Backend configuration
The configuration for the backend is split into multiple files for different environments:
| file | read order | environment | purpose |
| --- | --- | --- | --- |
| `config.yaml` | 1 | all | config definition |
| `config.development.yaml` | 2 | development | contains all necessary information for the development environment |
| `config.production.yaml` | 2 | production | some minor adjustments for production |
| `config.local.yaml` | 4 | all | excluded from GIT, serves as user specific overwrite |

The environment is set by the environment variable `NF_CLOUD_ENV`. The default environment is `development`.

You can overwrite some configuration variables and the environment with CLI arguments. For more information run `pipenv run python -m nf_cloud_backend --help`

### Frontend configuration
The frontend is configured by 3 environment variables:
| variable | default | description |
| --- | --- | --- |
| NF_CLOUD_BACKEND_BASE_URL | `http://localhost:3000` | Base URL for the backend (no trailing slash, must be accessible for the internet browser) |
| NF_CLOUD_FRONTEND_INTERFACE | `127.0.0.1` | IP for the frontend |
| NF_CLOUD_FRONTEND_PORT | `5000` | Port of the frontend |
## Production
You have two options to run the app:
* [Native deployment](#native-deployment)
* [Docker deployment](#docker-deployment)

### Native deployment
1. Clone the project
2. Create a configuration by copying `config.yaml` to `config.local` and adjust it to your needs.
3. Create a virtual Python environment by running `pipenv install`
4. Start the backend with `pipenv run python -m nf_cloud_backend`
5. Prepare the frontend dependencies `yarn --cwd ./nf_cloud_frontend install`
6. Prepare the frontend `yarn --cwd ./nf_cloud_frontend build`
7. Start the frontend `yarn --cwd ./nf_cloud_frontend start`

### Docker deployment
1. Clone the project
2. Create a configuration by copying `config.yaml` to `config.local` and adjust it to your needs.
3. Build the frontend image `docker build --tag="mpc/nf-cloud-backend:<some-version-tag>" -f backend.dockerfile .`
4. Build the backend image `docker build --tag="mpc/nf-cloud-frontend:<some-version-tag>" -f frontend.dockerfile .`
5. Start the backend container `docker run -d -it --name="nf-cloud-backend" -p <host-port>:<container-port> mpc/nf-cloud-backend:<some-version-tag>` (you can also add CLI arguments for the backend)
6. Start the frontend container `docker run -d -it --name="nf-cloud-frontend" -p <host-port>:<container-port> mpc/nf-cloud-frontend:<some-version-tag>`

### Make it available over a single domain and scale it up
It is recommended to use a reverse proxy like NginX to serve the frontend and backend over a single domain. Have look at `nginx-high-available.conf` how to set it up.  
`nginx-high-available.conf` is also configured to splitup and distribute incoming requests to the backend to three seperate backend instances. While the first process is only used to serve fast requests, the second and third instances are used for more complex search operation. Each upstream block can be scaled up by providing more backend instances.
