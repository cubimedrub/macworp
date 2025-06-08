#!/bin/sh

####
# Creates a self-signed certificate for quickstarting MacWorP.
#
# Arguments:
# $1 - Path to SSL directory
# $2 - Domain name
####

# Container var
MKCERT_CONTAINER=alpine/mkcert:latest
CONTAINER_MNT=/mnt
CERT_FILE=${CONTAINER_MNT}/cert.pem
KEY_FILE=${CONTAINER_MNT}/key.pem

# Host var
CUSTOM_SSL_DIR=$(realpath $1)
LAST_MACWORP_HOSTNAME_FILE=${CUSTOM_SSL_DIR}/last_macworp_hostname
USER_ID=$(id -u)
GROUP_ID=$(id -g)


DOMAIN=$2

create_certificate=false

# Cancel (re-) creation if MACWORP_HOSTNAME not set
if [ -z $1 ];
then
    echo 'No domain name set, canceling (re-) creation'
    create_certificate=false
fi

# Create custom certificate directory if not present
if [ ! -f $CUSTOM_SSL_DIR ]; 
then
    mkdir -p $CUSTOM_SSL_DIR
fi

# Schedule certificate creation if key and certificate file not exist
if [ ! -f $CERT_FILE -a ! -f $KEY_FILE ]; 
then
    echo 'no certificates found, creating some'
    create_certificate=true
fi

# Compare last MACWORP_HOSTNAME with current one and schedule recreation of certificates
# if hostnames don't match.
if [ -f $LAST_MACWORP_HOSTNAME_FILE ]; 
then
    last_macworp_hostname=$(cat $LAST_MACWORP_HOSTNAME_FILE)
    if [ "$last_macworp_hostname" = "$DOMAIN" ]; 
    then
        echo 'Domain changed, recreating certificates'
        create_certificate=true
    fi
fi


if [ "${create_certificate}" = "true" ]; 
then
    echo 'creating new certificates'
    rm -f ${CUSTOM_SSL_DIR}/*
    # Run mccer container without entrypoint to create certificates and correct permissions in one step.
    docker run \
        --rm -it --entrypoint "" \
        -v ${CUSTOM_SSL_DIR}:${CONTAINER_MNT} \
        $MKCERT_CONTAINER \
        sh -c "mkcert -key-file $KEY_FILE -cert-file $CERT_FILE $DOMAIN localhost; chown $USER_ID:$GROUP_ID $CERT_FILE $KEY_FILE"
    echo $DOMAIN > $LAST_MACWORP_HOSTNAME_FILE
fi
