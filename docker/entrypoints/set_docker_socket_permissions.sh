#!/bin/bash

#### Make sure mambauser can use the docker socket provided via bind mount

if [[ ! -e /var/run/docker.sock ]]
then
    echo "No docker socket present. Use a bind mount to add the host's docker socket \`docker run ... -v <HOST_DOCKER_SOCKET>:/var/run/docker.sock ...\`. Exiting..."
    exit 1
fi

# Get GID of docker socket
DOCKER_SOCKET_GID=$(ls -n /var/run/docker.sock | awk '{print $4}')
# Flag to create new docker group with matching GID
BUILD_NEW_DOCKER_GROUP=0

# Check if docker group exists
if cat /etc/group | grep -q docker;
then
    DOCKER_GID=$(cat /etc/group | grep docker | awk -F: '{print $4}')
    if [ $DOCKER_GID -eq $DOCKER_SOCKET_GID ]
    then
        usermod -aG docker mambauser
    else
        BUILD_NEW_DOCKER_GROUP=1
    fi
else
    BUILD_NEW_DOCKER_GROUP=1
fi

if [[ $BUILD_NEW_DOCKER_GROUP -ne 0 && $DOCKER_SOCKET_GID -ne 0 ]]
then
    groupadd -g $DOCKER_SOCKET_GID docker_host
    usermod -aG docker_host mambauser
fi

exec "$@"