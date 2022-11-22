#!/bin/sh

####
# Downloads mkcert binary, and creates self-signed certificates for testing 
# if env var NF_HOSTNAME is set.
####

CUSTOM_SSL_DIR=/etc/custom_ssl
CERT_FILE=${CUSTOM_SSL_DIR}/cert.pem
KEY_FILE=${CUSTOM_SSL_DIR}/key.pem
LAST_NF_HOSTNAME_FILE=${CUSTOM_SSL_DIR}/last_nf_hostname
MK_CERT_PATH=/usr/local/bin/mkcert

create_certificate=false

# Download mkcert if not present
if [ ! -f $MK_CERT_PATH ]; 
then
    echo 'downloading mkcert'
    curl -L -o $MK_CERT_PATH https://github.com/FiloSottile/mkcert/releases/download/v1.4.4/mkcert-v1.4.4-linux-"$(dpkg --print-architecture)"
    chmod +x $MK_CERT_PATH
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

# Compare last NF_HOSTNAME with current one and schedule recreation of certificates
# if hostnames don't match.
if [ -f $LAST_NF_HOSTNAME_FILE ]; 
then
    last_nf_hostname=$(cat $LAST_NF_HOSTNAME_FILE)
    if [ "$last_nf_hostname" = "$NF_HOSTNAME" ]; 
    then
        echo '$NF_HOSTNAME changed, recreating certificates'
        create_certificate=true
    fi
fi

# Cancel (re-) creation if NF_HOSTNAME not set
if [ -z $NF_HOSTNAME ];
then
    echo '$NF_HOSTNAME not set, canceling (re-) creation'
    create_certificate=false
fi

if [ "${create_certificate}" = "true" ]; 
then
    echo 'creating new certificates'
    rm -f /etc/custom_ssl/*
    $MK_CERT_PATH -key-file $KEY_FILE -cert-file $CERT_FILE $NF_HOSTNAME localhost
    echo $NF_HOSTNAME > $LAST_NF_HOSTNAME_FILE
fi
