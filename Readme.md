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
pipenv shell

# Upgrade pip and setuptools
pip install --upgrade pip setuptools

# Install dependencies
pipenv install -d

# Install node requirements
yarn --cwd ./nf_cloud_frontend install
```


### Start
```bash
# Shell 1
docker-compose up
# Shell 2
pipenv run db:migrate 'postgresql://postgres:developer@127.0.0.1:5434/nf_cloud'
pipenv run dev
```

| Component | Access | User | Password |
| --- | --- | --- | --- |
| Frontend | `http://localhost:5000` | | |
| API | `http://localhost:3001` | | |
| Fusionauth | `http://localhost:9011` | `developer@example.com` | `developer` |

For development, Flask is configured to add CORS-Headers by default.

### Database migrations
To keep track of database changes, this project uses [`peewee_migrate`](https://github.com/klen/peewee_migrate).
Migrations are located in `nf_cloud_backend/migrations/`. For now this needs to be added manually when using `pw_migrate`. 


## Production
Still work in progress.
### RabbitMQ
Set the `consumer_timeout` to a high value. Otherwise scheduled workflows can't be acknowledged after finishing and getting rescheduled.


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


