PUBLIC_REGISTRY := ghcr.io/cubimedrub
MACWORP_VERSION := 0.0.6

# Detect the operating system
ifeq ($(OS),Windows_NT)
DETECTED_OS := Windows
else
DETECTED_OS := $(shell sh -c 'uname 2>/dev/null || echo Unknown')
endif

# Deal with unknown OSs
ifeq ($(DETECTED_OS),Windows)
$(error Sorry Windows is not supported as many parts of MAcWorP are only usable on UNIX-like systems. Please try WSL.)
else ifeq ($(DETECTED_OS),Unknown)
$(error Unknown operating system.)
endif

# Set hostname depending on OS
ifeq ($(DETECTED_OS),Darwin)
MACWORP_HOSTNAME ?= $$(hostname)
else ifeq ($(DETECTED_OS),Linux)
MACWORP_HOSTNAME ?= $$(hostname --fqdn)
endif

# Set immutable variables
QUICKSTART_DIR=./quickstart
USER_ID=$$(id -u)
GROUP_ID=$$(id -g)

# CLI arguments
DOCKER_SOCKET_PATH ?= /var/run/docker.sock
PROJECT_DIR ?= $(QUICKSTART_DIR)/uploads
SSL_DIR ?= $(QUICKSTART_DIR)/ssl

# Set immutable variables depending on CLI arguments
ifeq ($(DETECTED_OS),Darwin)
# `realpath` on macOS throws an error when the path does not exist
PROJECT_DIR_ABSOLUTE:=$(shell python3 -c "from pathlib import Path; print(Path('${PROJECT_DIR}').absolute())")	
else ifeq ($(DETECTED_OS),Linux)
PROJECT_DIR_ABSOLUTE:=$(shell realpath ${PROJECT_DIR})
endif


NIGHTLY_REGISTRY := cubimedrub
NIGHTLY_VERSION := nightly

.SILENT:

#ghcr.io/cubimedrub/macworp-backend

# Production test
quickstart-up:
	# Create directories
	mkdir -p ${PROJECT_DIR_ABSOLUTE}
	mkdir -p ${SSL_DIR}

	# Download prebuild containers
	docker pull alpine/mkcert:latest
	docker pull "${PUBLIC_REGISTRY}/macworp-backend:${MACWORP_VERSION}"
	docker pull "${PUBLIC_REGISTRY}/macworp-worker:${MACWORP_VERSION}"
	docker pull "${PUBLIC_REGISTRY}/macworp-frontend:${MACWORP_VERSION}"

	# Generate SSL certificates for the hostname
	${QUICKSTART_DIR}/certificate-creation.sh ${SSL_DIR} ${MACWORP_HOSTNAME} 

	# Write the link to the project directory
	echo "https://${MACWORP_HOSTNAME}:16160" > ${QUICKSTART_DIR}/URL

	# Start docker-compose in separate docker-compose project called macworp-quickstart, combining the two docker-compose files
	env CONTAINER_REGISTRY=${PUBLIC_REGISTRY} CONTAINER_VERSION=${MACWORP_VERSION} DOCKER_SOCKET_PATH=${DOCKER_SOCKET_PATH} PROJECT_DIR_ABSOLUTE=${PROJECT_DIR_ABSOLUTE} SSL_DIR=${SSL_DIR} MACWORP_HOSTNAME=${MACWORP_HOSTNAME} MACWORP_FUSIONAUTH_PROTOCOL=https MACWORP_FUSIONAUTH_PORT=16161 \
		docker compose -p macworp-quickstart -f docker-compose.yml -f ${QUICKSTART_DIR}/docker-compose.yml up

quickstart-down:
	# Destroy production test
	env CONTAINER_REGISTRY=${PUBLIC_REGISTRY} CONTAINER_VERSION=${MACWORP_VERSION} DOCKER_SOCKET_PATH=${DOCKER_SOCKET_PATH} PROJECT_DIR_ABSOLUTE=${PROJECT_DIR_ABSOLUTE} SSL_DIR=${SSL_DIR} MACWORP_HOSTNAME=${MACWORP_HOSTNAME} MACWORP_FUSIONAUTH_PROTOCOL=https MACWORP_FUSIONAUTH_PORT=16161 \
		docker compose -p macworp-quickstart -f docker-compose.yml -f ${QUICKSTART_DIR}/docker-compose.yml down --remove-orphans

quickstart-bash:
	env CONTAINER_REGISTRY=${PUBLIC_REGISTRY} CONTAINER_VERSION=${MACWORP_VERSION} DOCKER_SOCKET_PATH=${DOCKER_SOCKET_PATH} PROJECT_DIR_ABSOLUTE=${PROJECT_DIR_ABSOLUTE} SSL_DIR=${SSL_DIR} MACWORP_HOSTNAME=${MACWORP_HOSTNAME} MACWORP_FUSIONAUTH_PROTOCOL=https MACWORP_FUSIONAUTH_PORT=16161 \
		docker compose -p macworp-quickstart -f docker-compose.yml -f ${QUICKSTART_DIR}/docker-compose.yml exec -it $(CONTAINER) bash

quickstart-logs:
	env CONTAINER_REGISTRY=${PUBLIC_REGISTRY} CONTAINER_VERSION=${MACWORP_VERSION} DOCKER_SOCKET_PATH=${DOCKER_SOCKET_PATH} PROJECT_DIR_ABSOLUTE=${PROJECT_DIR_ABSOLUTE} SSL_DIR=${SSL_DIR} MACWORP_HOSTNAME=${MACWORP_HOSTNAME} MACWORP_FUSIONAUTH_PROTOCOL=https MACWORP_FUSIONAUTH_PORT=16161 \
		docker compose -p macworp-quickstart -f docker-compose.yml -f ${QUICKSTART_DIR}/docker-compose.yml logs -f $(CONTAINER)


# Quickstart using nightly containers
quickstart-nightly-up:
	# Create directories
	mkdir -p ${PROJECT_DIR_ABSOLUTE}
	mkdir -p ${SSL_DIR}

	# Download prebuild containers
	docker pull alpine/mkcert:latest

	# Build docker container for backend, worker & frontend with the UID of the current user
	env DOCKER_BUILDKIT=1 docker build -t "${NIGHTLY_REGISTRY}/macworp-backend:${NIGHTLY_VERSION}" -f docker/backend.dockerfile .
	env DOCKER_BUILDKIT=1 docker build -t "${NIGHTLY_REGISTRY}/macworp-worker:${NIGHTLY_VERSION}" -f docker/worker.dockerfile .
	env DOCKER_BUILDKIT=1 docker build -t "${NIGHTLY_REGISTRY}/macworp-frontend:${NIGHTLY_VERSION}" -f docker/frontend.dockerfile .

	# Generate SSL certificates for the hostname
	${QUICKSTART_DIR}/certificate-creation.sh ${SSL_DIR} ${MACWORP_HOSTNAME} 

	# Write the link to the project directory
	echo "https://${MACWORP_HOSTNAME}:16160" > ${QUICKSTART_DIR}/URL

	# Start docker-compose in separate docker-compose project called macworp-quickstart, combining the two docker-compose files
	env CONTAINER_REGISTRY=${NIGHTLY_REGISTRY} CONTAINER_VERSION=${NIGHTLY_VERSION} DOCKER_SOCKET_PATH=${DOCKER_SOCKET_PATH} PROJECT_DIR_ABSOLUTE=${PROJECT_DIR_ABSOLUTE} SSL_DIR=${SSL_DIR} MACWORP_HOSTNAME=${MACWORP_HOSTNAME} MACWORP_FUSIONAUTH_PROTOCOL=https MACWORP_FUSIONAUTH_PORT=16161 \
		docker compose -p macworp-quickstart-nightly -f docker-compose.yml -f ${QUICKSTART_DIR}/docker-compose.yml up

quickstart-nightly-down:
	# Destroy production test
	env CONTAINER_REGISTRY=${NIGHTLY_REGISTRY} CONTAINER_VERSION=${NIGHTLY_VERSION} DOCKER_SOCKET_PATH=${DOCKER_SOCKET_PATH} PROJECT_DIR_ABSOLUTE=${PROJECT_DIR_ABSOLUTE} SSL_DIR=${SSL_DIR} MACWORP_HOSTNAME=${MACWORP_HOSTNAME} MACWORP_FUSIONAUTH_PROTOCOL=https MACWORP_FUSIONAUTH_PORT=16161 \
		docker compose -p macworp-quickstart-nightly -f docker-compose.yml -f ${QUICKSTART_DIR}/docker-compose.yml down --remove-orphans


quickstart-nightly-bash:
	env CONTAINER_REGISTRY=${NIGHTLY_REGISTRY} CONTAINER_VERSION=${NIGHTLY_VERSION} DOCKER_SOCKET_PATH=${DOCKER_SOCKET_PATH} PROJECT_DIR_ABSOLUTE=${PROJECT_DIR_ABSOLUTE} SSL_DIR=${SSL_DIR} MACWORP_HOSTNAME=${MACWORP_HOSTNAME} MACWORP_FUSIONAUTH_PROTOCOL=https MACWORP_FUSIONAUTH_PORT=16161 \
		docker compose -p macworp-quickstart-nightly -f docker-compose.yml -f ${QUICKSTART_DIR}/docker-compose.yml exec -it $(CONTAINER) bash

quickstart-nightly-logs:
	env CONTAINER_REGISTRY=${NIGHTLY_REGISTRY} CONTAINER_VERSION=${NIGHTLY_VERSION} DOCKER_SOCKET_PATH=${DOCKER_SOCKET_PATH} PROJECT_DIR_ABSOLUTE=${PROJECT_DIR_ABSOLUTE} SSL_DIR=${SSL_DIR} MACWORP_HOSTNAME=${MACWORP_HOSTNAME} MACWORP_FUSIONAUTH_PROTOCOL=https MACWORP_FUSIONAUTH_PORT=16161 \
		docker compose -p macworp-quickstart-nightly -f docker-compose.yml -f ${QUICKSTART_DIR}/docker-compose.yml logs -f $(CONTAINER)