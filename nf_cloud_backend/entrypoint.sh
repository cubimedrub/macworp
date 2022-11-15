#!/bin/bash

conda activate nf_cloud

echo "startet with ${@}"

# Prepend 'python ./run.py' arguments given by 'CMD' (dockerfile) or 'command' (docker-compose)
set -- python -m nf_cloud_backend "$@"
exec "$@"