FROM golang:1.11.2 AS minica

RUN apt-get update && apt-get install -y git
RUN go get github.com/jsha/minica

FROM nginx:stable

COPY  --from=minica /go/bin/minica /usr/local/bin/minica
ADD nginx/generate-certs-and-keys.sh /docker-entrypoint.d/
ADD nginx/template.conf /etc/nginx/conf.d/default.conf
ADD nginx/test.json /usr/share/nginx/html/test.json

