FROM golang:1.11.2 AS minica

RUN apt-get update && apt-get install -y git
RUN go get github.com/jsha/minica

FROM nginx:stable

COPY  --from=minica /go/bin/minica /usr/local/bin/minica
ADD nginx/generate-certs-and-keys.sh /docker-entrypoint.d/
RUN chmod +x /docker-entrypoint.d/generate-certs-and-keys.sh
ADD nginx/default.conf /etc/nginx/conf.d/default.conf

