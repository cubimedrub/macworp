# MAcWorP - Massive Accessible Workflow Platform

MAcWorP is a web application to give workflow developers a simple way to make their workflows easily accessible via a web browser for everyone. 

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

## Development

### Getting started
First shell
```sh
docker-compose up
```

Second shell
```sh
# Create environment
conda env create -f environment.yml

# Already created the environment and need updates?
conda env update -f environment.yml --prune

# Activate the new environment name might change in the future
conda activate macworp2 

# Initialize the database
python -m macworp db:init

# Optional: Add some data
python python -m macworp db:seed

honcho start
```
The last command will launch the backend, frontend & worker.


### Database access
```sh
psql postgresql://postgres:developer@127.0.0.1:5434/macworp

# ...or the test database
psql postgresql://postgres:developer@127.0.0.1:5434/macworp_test
```


## Other CLI options

### Initialize database
```sh
python -m macworp db:init
```
This is not a migration tool!

### Seed
```sh
python python -m macworp db:seed
```

### Start Backend
```sh
python -m macworp backend:start
```

### Start frontend
If debug is set to false, e.g. for production
```sh
python -m macworp frontend:start
```
for development
```sh
python src/macworp/__main__.py
```
otherwise NiceGUI is unhappy about spawning new processes when hot reloading


### Start worker
```sh
python -m macworp worker:start
```


### Test Backend
This is a big ToDo after the a major code revision

```sh
source test.env && touch $MACWORP_TEST_LOG && rm $MACWORP_TEST_LOG && python -m unittest discover macworp
```

### Architecture

#### Entity Component Diagram

![a](docs/entity-component-diagram.png)

### Permissions

For both projects and workflows, permissions act in similar ways. For brevity, "object" here refers to "project or workflow". In order of priority, the following rules apply:

* If the user has the `admin` role, any operation is permitted.
* If the user is set as the owner of the object, any operation is permitted. It is possible for projects to be "orphaned" if the user unregisters from the system. In this case, admin intervention is required.
* If the user has been given shared write access, they are allowed to view the object, as well as modify the object's properties and give out / remove shared privileges for other users. They may not delete the object, change its owner or its public link.
* If the user has been given shared read access, they can view the object, but not modify it.
* (Planned feature) Anyone, including unauthenticated users, can view the object via a guest link that can be enabled and configured by the owner.

### API conventions

#### Status codes

* 200 OK - general success
* 201 Created - success on creation
* 401 Unauthorized - user not authenticated
* 403 Forbidden - user authenticated but not permitted to perform operation
* 404 Not Found - route doesn't exist -- NOT when a requested resource doesn't exist
* 422 Unprocessable Content - general error
