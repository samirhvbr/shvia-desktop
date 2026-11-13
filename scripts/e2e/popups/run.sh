#!/usr/bin/env bash
# Native Linux acceptance. Copy artifacts into a disposable rootfs; never bind-mount them.
set -euo pipefail
[[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]] || { echo 'Native x86_64 Linux required' >&2; exit 2; }
here=$(cd "$(dirname "$0")" && pwd)
kind=${SHVIA_POPUP_RUN_KIND:-candidate}
case "$kind" in baseline|candidate) ;; *) exit 2;; esac
smoke=${SHVIA_POPUP_SMOKE_ONLY:-0}
case "$smoke" in 0|1) ;; *) exit 2;; esac
test -f "${SHVIA_DESKTOP_BINARY:?Point to the tested Desktop ELF}"
test -f "${SHVIA_ANNA_BINARY:?Point to its real staged Anna sidecar}"
app_version=${SHVIA_POPUP_APP_VERSION:?Specify the version of the tested build}
image=${SHVIA_POPUP_RUNTIME_IMAGE:-shvia-popup-e2e:local}
token=$(date -u +%Y%m%dT%H%M%SZ)-$$
container=shvia-popup-$kind-$token
network=shvia-popup-$token
output=${SHVIA_POPUP_OUTPUT_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/shvia-popup-$kind.XXXXXX")}
mkdir -p "$output"
container_created=0
network_created=0
cleanup() {
  local test_rc=$? container_removed=true network_removed=true
  if [[ "$container_created" == 1 ]]; then docker rm -f "$container" >/dev/null || container_removed=false; fi
  if [[ "$network_created" == 1 ]]; then docker network rm "$network" >/dev/null || network_removed=false; fi
  printf '{"containerRemoved":%s,"networkRemoved":%s,"coreLimit":0}\n' "$container_removed" "$network_removed" >"$output/cleanup.json"
  [[ "$container_removed" == true && "$network_removed" == true ]] || test_rc=2
  trap - EXIT
  exit "$test_rc"
}
trap cleanup EXIT
docker image inspect "$image" >/dev/null
docker network create --internal "$network" >/dev/null
network_created=1
docker create --name "$container" --network "$network" --cpus 1 --memory 2500m --ulimit core=0 --entrypoint /bin/sleep "$image" infinity >/dev/null
container_created=1
docker start "$container" >/dev/null
docker exec "$container" mkdir -p /fixture /artifacts /opt/shvia-fixture
for f in prepare.sh entrypoint.sh server.cjs controller.py; do docker cp "$here/context/$f" "$container:/opt/shvia-fixture/$f"; done
docker cp "$SHVIA_DESKTOP_BINARY" "$container:/artifacts/shvia-desktop"
docker cp "$SHVIA_ANNA_BINARY" "$container:/artifacts/anna"
ip=$(docker inspect "$container" --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}')
set +e
docker exec -e RUN_DIR=/fixture -e ORIGIN_HOST="$ip" -e RUN_KIND="$kind" -e SMOKE_ONLY="$smoke" \
  -e APP_VERSION="$app_version" -e EXPECTED_ANNA_VERSION="${SHVIA_POPUP_ANNA_VERSION:-}" \
  -e ORIGIN_OTHER_HOST="${SHVIA_POPUP_OTHER_HOST:-127.0.0.2}" \
  "$container" bash /opt/shvia-fixture/entrypoint.sh >"$output/run.log" 2>&1
rc=$?
set -e
docker inspect "$container" --format '{"id":"{{.Id}}","image":"{{.Image}}","privileged":{{.HostConfig.Privileged}},"binds":{{json .HostConfig.Binds}},"mounts":{{json .Mounts}},"capAdd":{{json .HostConfig.CapAdd}},"network":"{{.HostConfig.NetworkMode}}","publishedPorts":{{json .HostConfig.PortBindings}},"ulimits":{{json .HostConfig.Ulimits}}}' >"$output/isolation.json"
for f in acceptance-result.json server-state.json desktop.log external-open.log server.log; do
  if docker exec "$container" test -f "/fixture/$f"; then docker cp "$container:/fixture/$f" "$output/$f"; fi
done
while IFS= read -r f; do
  [[ "$f" =~ ^[A-Za-z0-9_.-]+\.png$ ]] || exit 2
  docker cp "$container:/fixture/$f" "$output/$f"
done < <(docker exec "$container" find /fixture -maxdepth 1 -type f -name '*.png' -printf '%f\n')
printf '{"testExitCode":%s,"smokeOnly":%s}\n' "$rc" "$([[ "$smoke" == 1 ]] && echo true || echo false)" >"$output/run-result.json"
test ! -f "$output/acceptance-result.json" || cat "$output/acceptance-result.json"
echo "Evidence: $output"
exit "$rc"
