# Installation

## Quickstart
Interested and want to try MAcWorP as fast as possible?   

1. A Unix-like operating system is mandatory: You are good to go with any popular Linux distribution, Windows Subsystem for Linux or macOS
2. Install [Docker](https://docs.docker.com/get-started/get-docker/)
    * On Linux make sure your user is allowed to access the Docker service by adding the user to the Docker group `usermod -aG docker <your-user>` and reboot
3. Install `make` & `git`
4. Clone the repository
5. `make quickstart-up`

This will start the web interface on `https://<local-computer-name>:16160`, the complete URL is written to the file `./quickstart/URL`. It might change depending on your location (home, office, ...) as it includes the computers FQDN. If you need a consistent domain name to be used, add `MACWORP_HOSTNAME=<domain-name-of-your-choice>` after the make command.

From here you can login using one of the following accounts:

| Name | Username | Password |
| --- | --- | --- |
| Dev: Login with local file | `developer` | `developer` |
| Dev: Login with local Fusionauth | `developer@example.com` | `developer` |

The next step is to create a project where data and results can be stored later.
Without any data uploaded, start one of the preinstalled `Result rendering` workflows.
This will generate a folder `results` with some example files which will be rendered in the browser once your go into the folder.

## Recommended full install

The installation instruction is a more detailed description of the quickstart setup. The prerequisites are located in `docker-compose.yml` while the MAcWorP specific container are located in `quickstart.docker-compose.yml` 

### Prerequisites
1. RabbitMQ (=3)
    * **Attention**: By default RabbitMQ's is setup to expect a message to be processed in a few seconds. As MAcWorP will probably run much longer jobs set `consumer_timeout` (ms) to something high e.g. one week. 
2. PostgreSQL (>=14)
3. Redis (=6)
This can also be deployed using Docker images.

If a distributed setup is intended, a shared file system is also needed. E.g. NFS seems to work quit good.

## Install
The recommended way of deploying MAcWorP is via the provided Docker containers.

1. Pull the images

    ```shell
    docker pull ghcr.io/cubimedrub/macworp-backend:latest
    docker pull ghcr.io/cubimedrub/macworp-worker:latest
    docker pull ghcr.io/cubimedrub/macworp-frontend:latest
    ```

2. Create a new config

    ```shell
    docker run --rm -ti ghcr.io/cubimedrub/macworp-backend utility config print > config.yml
    ```

    Configure it to your needs. Make sure to use paths within the container!
    It is possible to use environment variables by `ENV['<variable_name>']`

3. Create folder (on the shared filesystem) or volumes to persist projects files (uploads) and workflow sources
4. Start some backend container to distribute the load

    ```shell
    docker run -d -it \
        -n macworp-backend-<some_unique_suffix>
        -v $(pwd)/config.yml:/home/mambauser/config.yml:ro \
        -v <persistent_workflow_source_folder>:/workflows:ro \
        -v <persistent_upload_folder>:/projects \
        -e MACWORP_CONFIG=/home/mambauser/config.yml
        ghcr.io/cubimedrub/macworp-backend:latest \
        serve --gunicorn
    ```

    The first container should also run database migrations and RabbitMQ preparation by setting the environment variable: `-e RUN_MIGRATION=true`.

    **Note:** Mount your uploads/project directory at `/uploads`. During start the container tries to adjust the user within the container (executing user) to use use the same UID/GID as on the host.
    The container does *not* change the permission of the host folder itself.   
    For this to work a few things need to be considered:

    1. Recommended: Change the ownership to `non-root:non-root` and make the folder writable to the user and/or the group.
    2. Not recommended: If the ownership of the folder is `root:root`, the folder needs to be writable by the group `root` as the executing user cannot get the UID 0 but can be added to the root group. This is for example the case when using Docker Desktop or Docker for Mac as the background Linux VM is running the containers using root.
    3. Set the ownership of the config and the workflow folder to the same ownerships on the hosts

5. Next we need one frontend container

    ```shell
    docker run -d -it \
        -n macworp-frontend \
        -e MACWORP_BACKEND_BASE_URL"=<YOUR_DOMAIN_WITH_PROTOCOL>" \
        -e MACWORP_BACKEND_WS_URL="<YOUR_DOMAIN_WITH_PROTOCOL>" \
        -e MACWORP_FRONTEND_INTERFACE="0.0.0.0" \
        -e MACWORP_UPLOAD_MAX_FILE_SITE=10737418240 \
        -e MACWORP_DISABLE_SSL_VERIFICATION="true" \
        docker pull ghcr.io/cubimedrub/macworp-frontend:latest
    ```

    `<YOUR_DOMAIN_WITH_PROTOCOL>` could be something like `http://example.org` or with `https`.

6. The last piece of the puzzle to start the web services is NginX to combine everything in one service accessible via a single domain and port

    ```
    # Define a couple of backend containers to handle the live updates
    upstream socketio_handler {
        ip_hash;    # makes sure the same the channels are processed by the same proxy
        server macworp-backend-<some_unique_suffix>:3001;
        server ...;
    }

    # Depending on the expected user group add some backend container to handle file uploads,
    # log processing etc.
    upstream backend_handler {
        server macworp-backend-<some_other_unique_suffix>:3001;
        server ...;
    }

    # One frontend is sufficient
    upstream frontend_handler {
        server macworp-frontend:5001;
    }

    server {
        # http settings
        listen 80;
        server_name <YOUR_DOMAIN>;

        # https 
        #listen 443 ssl;
        #server_name <YOUR_DOMAIN>;

        #ssl_certificate     /etc/custom_ssl/cert.pem;
        #ssl_certificate_key /etc/custom_ssl/key.pem;
        #ssl_protocols       TLSv1 TLSv1.1 TLSv1.2;
        #ssl_ciphers         HIGH:!aNULL:!MD5;
        # end https

        # Adjust handling of client body 
        client_max_body_size 25G;
        client_body_buffer_size 100M;

        # Make files sending more efficient
        sendfile on;
        tcp_nopush on;

        add_header Access-Control-Allow-Origin *;
        add_header Access-Control-Allow-Methods *;
        add_header Access-Control-Expose-Headers Content-Type;

        # Deliver fast requests
        location ~ ^/api {
            # Disable buffering of upstream responses.
            proxy_buffering             off;
            # Set proxy headers for Flask
            proxy_set_header X-Real-IP  $remote_addr;
            proxy_set_header Host $http_host;
            proxy_set_header X-Forwarded-Proto "https";
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_redirect off;
            # Disable passing on to next upstreamserver
            # proxy_next_upstream off;
            # Adjust timeouts
            proxy_connect_timeout       20s;
            proxy_send_timeout          20s;
            proxy_read_timeout          20s;
            send_timeout                20s;
            proxy_pass http://backend_handler;
        }

        location /socket.io {
            add_header X-Debug "sock location";
            proxy_http_version 1.1;
            proxy_buffering off;
            proxy_redirect off;
            proxy_set_header Host $http_host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection "Upgrade";
            proxy_pass http://socketio_handler/socket.io;
        } 

        # Deliver frontend
        location / {
            # Disable buffering of upstream responses.
            proxy_buffering             off;
            # Set proxy headers for Flask
            proxy_set_header X-Real-IP  $remote_addr;
            proxy_set_header Host $http_host;
            proxy_set_header X-Forwarded-Proto $scheme;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_redirect off;
            proxy_pass http://frontend_handler;
        }
    }
    ```

    Save this to a file and start a NginX container

    ```shell
    docker run -d -it \
        -v <path_to_conf_file>:/etc/nginx/templates/default.conf.template:ro \
        nginx/nginx 
    ``` 

7. To start the worker run

    ```shell
    docker run -d -it \
        -v <persistent_workflow_source_folder>:/workflows:ro \
        -v <persistent_upload_folder>:<same_path_as_persistent_upload_folder_on_host> \
        -e PROJECTS_PATH=<same_path_as_persistent_upload_folder_on_host> \
        ghcr.io/cubimedrub/macworp-worker:latest \
        -vvvvvvvv \
        --skip-cert-verification \
        -n /usr/local/bin/nextflow \
        -s /opt/conda/envs/macworp/bin/snakemake \
        -c <YOUR_DOMAIN> \
        -r amqp://admin:developer@<IP_OF_RABBIT_HOST>:5672/%2f \
        -q project_workflow \
        -d <same_path_as_persistent_upload_folder_on_host> \
        -u worker \
        -p <change-to-the-same-password-as-in-the-backend-config> \
        --keep-intermediate-files
    ```

    **Note:**

    1. Like the backend, the worker container tries to adjust the executing user to use the same UID/GID as the owner of the the project directory. You have to consider the same rules.
    2. To be able to use Docker containers during the workflow execution the uploads/projects folder needs to be mounted at the exact same path as on the host and the environment variable `PROJECTS_DIR` needs to have the same path.