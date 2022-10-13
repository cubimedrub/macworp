#!/bin/bash
set -e

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<-EOSQL
    CREATE DATABASE nf_cloud;
    CREATE USER fusionauth PASSWORD 'developer';
	CREATE DATABASE fusionauth;
	GRANT ALL PRIVILEGES ON DATABASE fusionauth TO fusionauth;
EOSQL