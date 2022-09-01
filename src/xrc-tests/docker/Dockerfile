FROM golang:1.11.2 AS minica

RUN apt-get update && apt-get install -y git
RUN go get github.com/jsha/minica

FROM nginx:stable

# 
COPY  --from=minica /go/bin/minica /usr/local/bin/minica

# Install all of the generated files
ADD nginx/generate-certs-and-keys.sh /docker-entrypoint.d/
ADD nginx/default.conf /etc/nginx/conf.d/default.conf
ADD nginx/json/ /srv/

RUN chmod +x /docker-entrypoint.d/generate-certs-and-keys.sh

# Install dfx
RUN apt-get update && apt-get install -y jq curl
COPY dfx.json dfx.json
RUN DFX_VERSION="$(jq -cr .dfx dfx.json)" sh -ci "$(curl -fsSL https://sdk.dfinity.org/install.sh)"
