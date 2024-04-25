# NF-Cloud v2 based on FastAPI

## How to

### Prepare

```sh
# Create environment
conda env create -f environment.yml

# Already created the environment and need updates?
conda env update -f environment.yml --prune

# Activate environment
conda activate nf_cloud_v2
```

## Start

```
# (Shell 1)
# Start PostgreSQL
docker-compose up

# (Shell 2)
# Start server
honcho -e dev.env start
```

### Access DB

```sh
psql postgresql://postgres:developer@127.0.0.1:5434/nf_cloud
```

We haven't set up a migration system yet, but I think until we have an actual prototype going that's just gonna be a waste of time.
Until then, whenever the schema gets changed, we can wipe the DB:

```sh
# Removes the NF cloud container
docker rm $(docker ps -f "name=nf-cloud" -q)
```
