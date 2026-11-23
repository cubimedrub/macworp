FROM rust:1.89 AS build-stage

RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
RUN cargo binstall dioxus-cli

WORKDIR /usr/src/macworp
COPY . .

RUN cd packages/web \
    && dx build --release


FROM caddy:2

COPY docker/web/Caddyfile /etc/caddy/Caddyfile
COPY --from=build-stage /usr/src/macworp/target/dx/macpepdb_web/release/web/public /usr/share/caddy
