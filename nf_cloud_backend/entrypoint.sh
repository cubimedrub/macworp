#!/bin/bash

if [ "$RUN_MIGRATION" = "true" ]
then
    db_is_ready=1
    while [ $db_is_ready -gt 0 ]
    do
        echo "waiting for DB server"
        pg_isready -h $MIGRATION_DB_HOSTNAME -t 3
        db_is_ready=$?
    done
    python -m nf_cloud_backend database migrate
    python -m nf_cloud_backend utility rabbitmq prepare
fi

# If max-decoy web serve is called with option gunicorn build params and pass it to gunicorn
# otherwise pass parameter to max-decoy.
if [[ "$1" == *"serve"* ]] && [[ "$@" == *"--gunicorn"* ]]
then
    gunicorn_args="$(python -m nf_cloud_backend "$@")"
    set -- gunicorn "$gunicorn_args"
    echo "starting: ${@}"
    eval "$@"
else
    set -- python -m nf_cloud_backend "$@"
    echo "starting: ${@}"
    exec "$@"
fi
