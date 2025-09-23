#!/bin/bash

if [[ ! -z "$RUN_MIGRATION" ]] && [[ -f "$RUN_MIGRATION"  ]]
then
    db_is_ready=1
    while [ $db_is_ready -gt 0 ]
    do
        echo "waiting for DB server"
        python -m macworp --config $RUN_MIGRATION db:is-connectable
        db_is_ready=$?
    done
    python -m macworp --config $RUN_MIGRATION db:init

    rabbitmq_is_ready=1
    while [ $rabbitmq_is_ready -gt 0 ]
    do
        echo "waiting for RabbitMQ server"
        python -m macworp --config $RUN_MIGRATION rabbitmq:is-connectable
        rabbitmq_is_ready=$?
    done
    python -m macworp --config $RUN_MIGRATION rabbitmq:init
fi

if [[ ! -z "$RUN_SEED" ]] && [[ -f "$RUN_SEED"  ]]
then
    db_is_ready=1
    while [ $db_is_ready -gt 0 ]
    do
        echo "waiting for DB server"
        python -m macworp --config $RUN_SEED db:is-connectable
        db_is_ready=$?
    done
    python -m macworp --config $RUN_SEED db:seed
fi

exec "$@"
