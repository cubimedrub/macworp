FROM rust:1.89 AS build-stage

WORKDIR /usr/src/macworp
COPY . .

RUN cd packages/worker \
    && cargo build -r


FROM ubuntu:bookworm

COPY --from=build-stage --chown=root:root --chmod=555 /usr/src/macworp/target/release/macworp_worker /usr/bin/macworp_worker
