# NF-Cloud v2 based on FastAPI

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

# Add some data
python python -m macworp db:seed

honcho start
```

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


### Test Backend
This is a big ToDo after the a major code revision

```sh
source test.env && touch $MACWORP_TEST_LOG && rm $MACWORP_TEST_LOG && python -m unittest discover macworp
```