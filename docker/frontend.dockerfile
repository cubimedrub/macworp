FROM node:18-bookworm

WORKDIR /app

COPY ./frontend /app
COPY ./docker/entrypoints/frontend.sh /app/entrypoint.sh

# Native installs
RUN apt-get update -y \
    && apt-get install -y curl \
    # Remove caches
    && rm -rf /var/lib/apt/lists/* 

RUN yarn install

ENTRYPOINT [ "./entrypoint.sh" ]
