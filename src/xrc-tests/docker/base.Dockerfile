FROM golang:1.11.2 AS minica

RUN apt-get update && apt-get install -y git
RUN go get github.com/jsha/minica

FROM nginx:stable

# Move minica over. 
COPY  --from=minica /go/bin/minica /usr/local/bin/minica

RUN apt-get update && apt-get install -y jq curl vim supervisor

# Install dfx
WORKDIR /work
ADD /src/xrc/xrc.did /work/src/xrc/xrc.did
ADD /dfx.json /work/dfx.json
RUN DFX_VERSION="$(jq -cr .dfx dfx.json)" sh -ci "$(curl -fsSL https://sdk.dfinity.org/install.sh)"
# Make a default identity
RUN dfx identity get-principal

# Install supervisord
RUN mkdir -p /var/log/supervisor
COPY /src/xrc-tests/docker/supervisord.conf /etc/supervisor/conf.d/supervisord.conf
COPY /src/xrc-tests/docker/docker-entrypoint.sh /docker-entrypoint.sh
RUN chmod +x /docker-entrypoint.sh 

ENTRYPOINT ["/docker-entrypoint.sh"]

CMD ["supervisord"]
