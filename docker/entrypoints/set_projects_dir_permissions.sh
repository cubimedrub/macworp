#!/bin/bash

#### Make sure mambauser has the same UID/GID as the user owning the project directory via bind mount

if [[ ! -d $PROJECTS_DIR ]]
then
    echo "Project directory not found. Please mount it via \`docker run ... -v <VOLUME_OR_HOST_DIRECTORY>:/projects ...\`. Exiting..."
    exit 1
fi

PROJECT_DIR_UID=$(ls -lan $PROJECTS_DIR | head -2 | tail -1 | awk '{print $3}')
PROJECT_DIR_GID=$(ls -lan $PROJECTS_DIR | head -2 | tail -1 | awk '{print $4}')

# If GID is not 0 (root) or 1000 (mambauser), change it
if [[ $PROJECT_DIR_UID -ne 0 && $PROJECT_DIR_UID -ne 1000 ]]
then
    usermod --login=mambauser --home=/home/mambauser \
        --move-home -u ${PROJECT_DIR_UID} ${MAMBA_USER}
fi

case $PROJECT_DIR_UID in
    0)
        # Add mambauser to root group
        usermod -aG root mambauser
        ;;
    $(id -u ${MAMBA_USER}))
        # Do nothing 
        ;;
    *)
        # Change GID of mambauser-group
        groupmod "-g ${PROJECT_DIR_GID}" "${MAMBA_USER}"
        ;;
esac

# Check if mambauser has write permissions to the project directory
TEST_FILE=$PROJECTS_DIR/.macworp_write_test_file

su $MAMBA_USER -c "echo 'do i have write access?' > $TEST_FILE"
if [[ $? -ne 0 ]]
then
    echo "The mambauser does not have write access to the project directory."
    echo "Make sure the project folder is writable by the owning group or when not owned by root by the owning user."
    echo "Exiting..."
    exit 1
else
    rm $TEST_FILE
fi



exec "$@"