FROM node:14-buster

WORKDIR /app

COPY ./frontend /app
COPY ./docker/entrypoints/frontend.sh /app/entrypoint.sh

RUN yarn install

ENTRYPOINT [ "./entrypoint.sh" ]
