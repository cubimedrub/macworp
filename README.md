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

### Start

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

# ...or the test database
psql postgresql://postgres:developer@127.0.0.1:5434/nf_cloud_test
```

Migration has been implemented through model imports in database.py and main.py

```sh
# Removes the NF cloud container
docker rm $(docker ps -f "name=nf-cloud" -qa)
```

### Start Backend

```sh
uvicorn nf_cloud_backend.main:app --app-dir backend --reload
```

### Seed DB

```
python -m nf_cloud_backend seed ./backend/db_seed.yaml
```

### Test Backend

```sh
source test.env && touch $MACWORP_TEST_LOG && rm $MACWORP_TEST_LOG && python -m unittest discover nf_cloud_backend
```