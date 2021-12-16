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
# Install the correct python version
pyenv install $(cat .python-version)

# Create an environment
pipenv install -d

# Install node requirements
yarn --cwd ./nf_cloud_frontend install
```


### Start
```bash
# Shell 1
docker-compose up
# Shell 2
pipenv run dev
```

| Component | Access |
| --- | --- |
| Frontend | `http://localhost:5000` |
| API | `http://localhost:3001` |
For development, Flask is configured to add CORS-Headers by default.

## Production
Coming soon


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


