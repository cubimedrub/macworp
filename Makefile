# Detect the operating system
ifeq ($(OS),Windows_NT)
DETECTED_OS := Windows
else
DETECTED_OS := $(shell sh -c 'uname 2>/dev/null || echo Unknown')
endif

# Deal with unknown OSs
ifeq ($(DETECTED_OS),Windows)
$(error Sorry Windows is not supported as many parts of MAcWorP are not compatible with Windows. Please try WSL.)
else ifeq ($(DETECTED_OS),Unknown)
$(error Unknown operating system.)
endif

# Set hostname depending on OS
ifeq ($(DETECTED_OS),Darwin)
MACWORP_HOSTNAME=$$(hostname)
else ifeq ($(DETECTED_OS),Linux)
MACWORP_HOSTNAME=$$(hostname --fqdn)
endif

# CLI arguments
DOCKER_SOCKET_PATH ?= /var/run/docker.sock
PROJECT_DIR ?= ./quickstart_uploads


# Set immutable variables
ifeq ($(DETECTED_OS),Darwin)
# `realpath` on macOS throws an error when the path does not exist
PROJECT_DIR_ABSOLUTE:=$(shell python3 -c "from pathlib import Path; print(Path('${PROJECT_DIR}').absolute())")	
else ifeq ($(DETECTED_OS),Linux)
PROJECT_DIR_ABSOLUTE:=$(shell realpath ${PROJECT_DIR})
endif
USER_ID=$$(id -u)
GROUP_ID=$$(id -g)

.SILENT:

# Production test
quickstart-up:
	# Create separate upload directory
	mkdir -p ${PROJECT_DIR_ABSOLUTE}
	# Build docker container for backend, worker & frontend with the UID of the current user
	env DOCKER_BUILDKIT=1 docker build -t "cubimedrub/macworp-backend:local" -f docker/backend.dockerfile .
	env DOCKER_BUILDKIT=1 docker build -t "cubimedrub/macworp-worker:local" -f docker/worker.dockerfile .
	env DOCKER_BUILDKIT=1 docker build -t "cubimedrub/macworp-frontend:local" -f docker/frontend.dockerfile .
	# Write the link to the project directory
	echo "https://${MACWORP_HOSTNAME}:16160" > QUICKSTART_URL
	# Start docker-compose in separate docker-compose project called macworp-quickstart, combining the two docker-compose files
	env DOCKER_SOCKET_PATH=${DOCKER_SOCKET_PATH} PROJECT_DIR_ABSOLUTE=${PROJECT_DIR_ABSOLUTE} MACWORP_HOSTNAME=${MACWORP_HOSTNAME} MACWORP_FUSIONAUTH_PROTOCOL=https MACWORP_FUSIONAUTH_PORT=16161 \
		docker compose -p macworp-quickstart -f docker-compose.yml -f quickstart.docker-compose.yml up ${args}

quickstart-down:
	# Destroy production test
	env DOCKER_SOCKET_PATH=${DOCKER_SOCKET_PATH} PROJECT_DIR_ABSOLUTE=${PROJECT_DIR_ABSOLUTE} MACWORP_HOSTNAME=${MACWORP_HOSTNAME} MACWORP_FUSIONAUTH_PROTOCOL=https MACWORP_FUSIONAUTH_PORT=16161 \
		docker compose -p macworp-quickstart -f docker-compose.yml -f quickstart.docker-compose.yml down
