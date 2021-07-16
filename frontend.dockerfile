FROM node:14-buster

WORKDIR /app

COPY ./nf_cloud_frontend /app

RUN yarn install

ENTRYPOINT [ "./entrypoint.sh" ]
