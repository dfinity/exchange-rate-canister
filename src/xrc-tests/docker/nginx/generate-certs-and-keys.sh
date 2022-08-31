#!/usr/bin/env bash

domains=("test.com" "test2.com")
# TODO: dump domains into script

mkdir /certs
cd /certs

for domain in ${domains[@]}; do
    minica --domains "${domain}"
done
ls -la

mkdir -p /etc/nginx/certs
mv minica.pem /usr/local/share/ca-certificates/minica.crt
mv /certs/* /etc/nginx/certs
update-ca-certificates

# TODO: dump the domains into the host file
