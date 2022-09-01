FROM golang:1.11.2 AS minica

RUN apt-get update && apt-get install -y git
RUN go get github.com/jsha/minica

FROM nginx:stable

# Move minica over. 
COPY  --from=minica /go/bin/minica /usr/local/bin/minica

# Install dfx
RUN apt-get update && apt-get install -y jq curl
WORKDIR /work
ADD /src/xrc/xrc.did /work/src/xrc/xrc.did
ADD /dfx.json /work/dfx.json
ADD /src/xrc-tests/gen/xrc.wasm /work/xrc.wasm
RUN DFX_VERSION="$(jq -cr .dfx dfx.json)" sh -ci "$(curl -fsSL https://sdk.dfinity.org/install.sh)"
