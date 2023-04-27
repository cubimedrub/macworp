FROM node:14-buster

WORKDIR /app

COPY ./frontend /app

RUN yarn install

ENTRYPOINT [ "./entrypoint.sh" ]
