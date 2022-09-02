FROM golang:1.11.2 AS minica

RUN apt-get update && apt-get install -y git
RUN go get github.com/jsha/minica

FROM nginx:stable

# Move minica over. 
COPY  --from=minica /go/bin/minica /usr/local/bin/minica

# Install dfx
RUN apt-get update && apt-get install -y jq curl wget vim
WORKDIR /work
ADD /src/xrc/xrc.did /work/src/xrc/xrc.did
ADD /dfx.json /work/dfx.json
RUN DFX_VERSION="$(jq -cr .dfx dfx.json)" sh -ci "$(curl -fsSL https://sdk.dfinity.org/install.sh)"
# Make a default identity
RUN dfx identity get-principal
