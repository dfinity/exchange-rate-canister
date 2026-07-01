FROM golang:1.20.0 AS minica

RUN apt-get update && apt-get install -y git
RUN go install github.com/jsha/minica@latest

FROM ubuntu:24.04

ARG DEBIAN_FRONTEND=noninteractive

# Move minica over. 
COPY  --from=minica /go/bin/minica /usr/local/bin/minica
RUN apt-get update && apt-get install -y \
    curl \
    gnupg \
    && curl -fsSL https://openresty.org/package/pubkey.gpg | gpg --dearmor -o /usr/share/keyrings/openresty.gpg \
    && echo "deb [signed-by=/usr/share/keyrings/openresty.gpg] http://openresty.org/package/ubuntu noble main" \
       > /etc/apt/sources.list.d/openresty.list \
    && apt-get update \
    && apt-get install -y \
    openresty \
    jq \
    curl \
    vim \
    supervisor \
    libunwind-dev \
    unzip \
    && mkdir -p /etc/nginx/conf.d /var/log/nginx \
    && ln -s /usr/local/openresty/nginx/sbin/nginx /usr/bin/nginx

# Install dfx
WORKDIR /work
ADD /src/xrc/xrc.did /work/src/xrc/xrc.did
ADD /dfx.json /work/dfx.json

ENV PATH="/root/.local/share/dfx/bin:${PATH}"
# The e2e container deliberately runs a newer dfx than the project default in
# dfx.json: its PocketIC-backed local network provides a current replica,
# while the release build (top-level Dockerfile) keeps the pinned dfx. The
# DFX_VERSION variable overrides dfx.json for every dfx invocation in here.
ENV DFX_VERSION=0.32.0
# Keep dfx's stderr clean: the harness treats unexpected stderr output as an error.
ENV DFX_WARNING=-deprecation
RUN DFXVM_INIT_YES=true sh -c "$(curl -fsSL https://sdk.dfinity.org/install.sh)" && dfx --version
# Make a default identity
RUN dfx identity get-principal

RUN dfx start --background && dfx stop

# Install supervisord
RUN mkdir -p /var/log/supervisor
COPY /src/xrc-tests/docker/supervisord.conf /etc/supervisor/conf.d/supervisord.conf
COPY /src/xrc-tests/docker/docker-entrypoint.sh /docker-entrypoint.sh
RUN chmod +x /docker-entrypoint.sh

ADD /src/xrc-tests/docker/router.lua /etc/nginx/router.lua
COPY /src/xrc-tests/docker/nginx.conf /usr/local/openresty/nginx/conf/nginx.conf

ENTRYPOINT ["/docker-entrypoint.sh"]

CMD ["supervisord"]
