FROM golang:1.20.0 AS minica

RUN apt-get update && apt-get install -y git
RUN go install github.com/jsha/minica@latest

FROM ubuntu:22.04

ARG DEBIAN_FRONTEND=noninteractive

# Move minica over. 
COPY  --from=minica /go/bin/minica /usr/local/bin/minica

RUN apt-get update && apt-get install -y \
    nginx \
    libnginx-mod-http-echo \
    libnginx-mod-http-lua \
    jq \
    curl \
    vim \
    supervisor \
    libunwind-dev \
    unzip

# Install dfx
WORKDIR /work
ADD /src/xrc/xrc.did /work/src/xrc/xrc.did
ADD /dfx.json /work/dfx.json

ENV PATH="/root/.local/share/dfx/bin:${PATH}"
RUN DFXVM_INIT_YES=true DFX_VERSION="$(jq -cr .dfx dfx.json)" sh -c "$(curl -fsSL https://sdk.dfinity.org/install.sh)" && dfx --version
# Make a default identity
RUN dfx identity get-principal

RUN dfx start --background && dfx stop

# Install supervisord
RUN mkdir -p /var/log/supervisor
COPY /src/xrc-tests/docker/supervisord.conf /etc/supervisor/conf.d/supervisord.conf
COPY /src/xrc-tests/docker/docker-entrypoint.sh /docker-entrypoint.sh
RUN chmod +x /docker-entrypoint.sh

ADD /src/xrc-tests/docker/router.lua /etc/nginx/router.lua

ENTRYPOINT ["/docker-entrypoint.sh"]

CMD ["supervisord"]
