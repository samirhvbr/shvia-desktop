#!/usr/bin/env bash
set -euo pipefail
[[ "${RUN_DIR:?}" == /fixture && -f /.dockerenv ]] || exit 2
mkdir -p /fixture/home/.config/cloud.blue3.shvia /fixture/home/.cache /fixture/home/.local/share /fixture/runtime /fixture/bin
chmod 700 /fixture/runtime
umask 077
openssl req -x509 -newkey rsa:2048 -nodes -days 2 -keyout /fixture/ca.key -out /fixture/ca.crt -subj '/CN=SHVIA isolated popup regression CA' -addext 'basicConstraints=critical,CA:TRUE' -addext 'keyUsage=critical,keyCertSign,cRLSign' > /fixture/openssl.log 2>&1
openssl req -newkey rsa:2048 -nodes -keyout /fixture/server.key -out /fixture/server.csr -subj "/CN=${ORIGIN_HOST:?}" >> /fixture/openssl.log 2>&1
printf 'subjectAltName=IP:%s,IP:127.0.0.2,DNS:localhost\nbasicConstraints=critical,CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n' "$ORIGIN_HOST" > /fixture/server.ext
openssl x509 -req -in /fixture/server.csr -CA /fixture/ca.crt -CAkey /fixture/ca.key -CAcreateserial -days 2 -extfile /fixture/server.ext -out /fixture/server.crt >> /fixture/openssl.log 2>&1
printf '{"url":"https://%s:8443"}\n' "$ORIGIN_HOST" > /fixture/home/.config/cloud.blue3.shvia/server.json
cat > /fixture/bin/xdg-open <<'STUB'
#!/bin/sh
printf '%s\n' "$*" >> /fixture/external-open.log
STUB
chmod 755 /fixture/bin/xdg-open
: > /fixture/external-open.log
cp /fixture/ca.crt /usr/local/share/ca-certificates/shvia-popup-regression.crt
chmod 644 /usr/local/share/ca-certificates/shvia-popup-regression.crt
update-ca-certificates >/fixture/ca-install.log 2>&1
