#!/usr/bin/env bash
# Runs the Traffic end-to-end tests (src-tauri/src/code_bridge/traffic.rs, the `#[ignore]`d ones)
# against the REAL traffic-capture sidecar and REAL packets, without sudo and without touching a real
# interface: inside `unshare -Urn` the test is root of a brand-new network namespace, where it owns
# CAP_NET_RAW over that namespace's own `lo` and nothing else.
#
# Needs: Linux with unprivileged user namespaces, `ip` and `python3`, and libpcap development files
# (apt install libpcap-dev; or LIBPCAP_LIBDIR=<dir with libpcap.so> LIBPCAP_VER=1.10.x).
# Not part of CI: a hosted runner may forbid user namespaces, and a test that silently skips there
# would look green. Run it by hand before touching the sidecar or code_bridge/traffic.rs.
set -euo pipefail
cd "$(dirname "$0")/.."

unshare -Urn true 2>/dev/null || { echo "this machine does not allow unprivileged user namespaces (unshare -Urn)"; exit 2; }
cargo build --release --locked --manifest-path src-tauri/traffic-capture/Cargo.toml
# The debug test build looks for the sidecar in traffic-capture/target/release (resolve_sidecar).
test_binary="$(cargo test --locked --manifest-path src-tauri/Cargo.toml --lib --no-run --message-format=json \
  | python3 -c '
import json, sys
path = None
for line in sys.stdin:
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") == "compiler-artifact" and m.get("profile", {}).get("test") and m.get("executable"):
        path = m["executable"]
print(path or "")')"
[ -n "$test_binary" ] || { echo "could not find the test binary"; exit 1; }
exec unshare -Urn bash -c "ip link set lo up && exec '$test_binary' --ignored --nocapture traffic::tests"
