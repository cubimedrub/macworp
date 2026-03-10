FROM cubimedrub/macworp-dev-v2:nightly
LABEL maintainer="dirk.winkelhardt@rub.de"

# 1. Set up mambauser to access the docker socket
# 2. Set up mambauser to access the projects directory
# 3. Change the user to mambauser
# 4. Activate the conda environment
# 5. Check if migration and seeding is needed 
# 6. Start the macworp *
ENTRYPOINT [ "/usr/local/bin/set_docker_socket_permissions.sh", "/usr/local/bin/set_projects_dir_permissions.sh", "/usr/local/bin/change_mambauser.sh", "/usr/local/bin/_entrypoint.sh", "/usr/local/bin/migration_and_seed.sh", "bash" ]
