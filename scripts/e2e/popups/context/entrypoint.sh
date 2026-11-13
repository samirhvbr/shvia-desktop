#!/usr/bin/env bash
set -euo pipefail
[[ -f /.dockerenv && "${RUN_DIR:?}" == /fixture && "$(uname -m)" == x86_64 ]] || exit 2
ulimit -c 0
bash /opt/shvia-fixture/prepare.sh
install -m 755 /artifacts/shvia-desktop /usr/bin/shvia-desktop
install -m 755 /artifacts/anna /usr/bin/anna
actual_anna=$(/usr/bin/anna --version)
expected_anna=${EXPECTED_ANNA_VERSION:-$actual_anna}
[[ "$actual_anna" == "$expected_anna" ]]
chown -R builder:builder /fixture
exec timeout --kill-after=10 480 runuser -u builder -- env -i HOME=/fixture/home XDG_CONFIG_HOME=/fixture/home/.config XDG_CACHE_HOME=/fixture/home/.cache XDG_DATA_HOME=/fixture/home/.local/share XDG_RUNTIME_DIR=/fixture/runtime RUN_DIR=/fixture ORIGIN_HOST="$ORIGIN_HOST" RUN_KIND="${RUN_KIND:-candidate}" APP_VERSION="${APP_VERSION:?}" EXPECTED_ANNA_VERSION="$expected_anna" SMOKE_ONLY="${SMOKE_ONLY:-0}" ORIGIN_OTHER_HOST="${ORIGIN_OTHER_HOST:-127.0.0.2}" SHVIA_RUNNERS_AUTO=0 LANG=C.UTF-8 GIO_USE_PROXY_RESOLVER=dummy PATH=/fixture/bin:/usr/local/bin:/usr/bin:/bin GDK_BACKEND=x11 WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1 WEBKIT_DISABLE_DMABUF_RENDERER=1 LIBGL_ALWAYS_SOFTWARE=1 OPEN_LOG=/fixture/external-open.log BROWSER=/fixture/bin/xdg-open dbus-run-session -- xvfb-run -a -s '-screen 0 1600x1000x24' bash -c '
 openbox --sm-disable >/fixture/window-manager.log 2>&1 &
 node /opt/shvia-fixture/server.cjs >/fixture/server.log 2>&1 &
 ready=0
 for attempt in $(seq 1 100); do
   if curl --noproxy "*" -fsS --cacert /fixture/ca.crt "https://$ORIGIN_HOST:8443/status" >/dev/null 2>&1; then ready=1;break;fi
   sleep .1
 done
 [ "$ready" = 1 ] || exit 2
 python3 /opt/shvia-fixture/controller.py
 '
