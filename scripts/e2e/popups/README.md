# Native popup acceptance

This fixture checks the native window and bridge behavior with real UI gestures. It uses no account, model request, production API or user files. Build/signing, installer/update acceptance and deployment remain separate checks.

Approved requests create independent canonical ShvIA windows. The original WebKit popup is cancelled: `window.open` returns `null`, and the new window has `window.opener === null`. Scripts cannot use the returned `WindowProxy` or opener messaging. Requested `width=100,height=100,left=0,top=0` must preserve normal native geometry, at least 800×600.

## Linux: isolated automated run

Run on native x86_64 Linux with Docker. The Dockerfile uses public Debian packages and contains only runtime/UI test tools. Provide the actual Desktop ELF and its real staged Anna sidecar; inert CI stand-ins do not prove native engine execution.

From the repository root:

```sh
docker build -t shvia-popup-e2e:local scripts/e2e/popups

SHVIA_DESKTOP_BINARY=/path/to/shvia-desktop \
SHVIA_ANNA_BINARY=/path/to/anna \
SHVIA_POPUP_APP_VERSION="$(cat version.md)" \
SHVIA_POPUP_OUTPUT_DIR=/tmp/shvia-popup-candidate \
bash scripts/e2e/popups/run.sh
```

The runner copies inputs into a fresh container: no bind mounts, Docker socket, host display, published ports or extra capabilities. An internal Docker bridge serves the approved origin and three real alternate destinations. Only the disposable rootfs trusts its generated CA. WebKit uses Xvfb and software rendering; its sandbox-disable flag is scoped to this container. Core dumps are disabled. The controller asks Openbox to close the exact client with a WM_DELETE_WINDOW request (wmctrl), exercising the native close handler without destroying the X window directly. The EXIT trap removes the container/network and writes `cleanup.json`.

Options:

- `SHVIA_POPUP_RUN_KIND=baseline` labels an unchanged-base run. Reproduced failures return nonzero.
- `SHVIA_POPUP_SMOKE_ONLY=1` runs only the two approved popup/bridge/geometry/close cases; the receipt explicitly marks the reduced scope.
- `SHVIA_POPUP_RUNTIME_IMAGE` selects another prepared image.
- `SHVIA_POPUP_ANNA_VERSION` optionally checks the staged sidecar's `--version` string. Otherwise it is read inside the container and compared with the actual bridge result.
- `SHVIA_POPUP_OTHER_HOST` defaults to `127.0.0.2`; `localhost` is also covered by the fixture certificate.

Full acceptance requires:

1. Eight trusted clicks: `target=_blank` and synchronous `window.open` for approved origin, other port, scheme and host. Anchors have no `onclick`. A passive observer records `isTrusted`, user activation and `defaultPrevented` after dispatch. It never cancels the event.
2. Popup gestures also require active user activation when the API is available. Both approved children answer real native `engineStatus('gateway')`, have no opener, keep the same dimensions as the fresh main window, with native geometry ≥ 800×600 and leave the parent responsive after close. `window.open` reports `returnedNull=true`.
3. Both children attempt all three external navigations. Each must invoke the external opener and retain its working bridge.
4. Each child opens an approved nested window using its original method. The nested bridge/geometry/isolation and the parent's responsiveness after closing it must pass.
5. Six external popup attempts invoke the external opener, create no native window and reach no unapproved native document. All four destination servers are health-checked first, so a missing destination cannot produce a pass.

HTTP and HTTPS are both served on 8443 through a fixture TCP proxy; the scheme test changes only the scheme. The host test changes only the host. Status preflights are excluded from navigation-request counts. All updater endpoints return 204.

Evidence includes binary SHA-256, real native responses, event/default-action records, X11 geometry, opener URLs, screenshots, process health, isolation settings and cleanup results. Home, browser profile and CA private key are never exported. Window positions are recorded; exact pixel placement is controlled by the window manager and is not asserted. The Python controller is Linux/X11-specific; Windows UI is not covered.

## macOS: same fixture, manual native acceptance

Use a disposable macOS VM and test user with no real accounts or folders. Do not change a real host's trust store, default browser or network aliases. Use the same server and payloads; macOS UI input/window ownership must be observed separately from the Linux controller.

1. Copy `context/server.cjs` and `preflight.cjs` into the VM. Create a private temporary fixture directory containing `ca.crt`, `server.crt`, `server.key` and an empty `external-open.log`. Generate a temporary CA/server certificate with SANs for the VM's non-loopback IPv4 address and `DNS:localhost`. Trust that CA only in the disposable VM/test keychain.
2. Set the disposable user's `~/Library/Application Support/cloud.blue3.shvia/server.json` to `{"url":"https://<VM-IP>:8443"}`. Do not copy an existing browser profile.
3. Start the server with `RUN_DIR=<fixture> ORIGIN_HOST=<VM-IP> ORIGIN_OTHER_HOST=localhost APP_VERSION=<tested-version> node server.cjs`. The loopback address127.0.0.2 is not automatically assigned on macOS; use the `localhost` host case without adding an interface alias.
4. Before opening the app, run `RUN_DIR=<fixture> node preflight.cjs`. Require all four 200/received-origin checks. Keep `destination-preflight.json`.
5. Start the exact unsigned/signed candidate and note its SHA/version. Click actual controls, using the mouse or native UI automation; do not call `element.click()` or `window.open` from an automation console. Require the event records to show `isTrusted=true`, active user gesture and no cancelled default.
6. Run the same 8+6+2 matrix above. The main page preserves all eight IDs. A child lists `child-port`, `child-scheme`, `child-host`, then appends `nested-blank` and `nested-open`. Require `hasOpener=false`, real native version replies, native dimensions and parent responsiveness after closing nested/child windows.
7. Confirm external destinations belong to Safari/the system browser and never a ShvIA native window. External-browser reports must have no native bridge. Safari may legitimately request those pages; distinguish these from unauthorized documents inside ShvIA. Record application-owned window counts and screenshots.

`server-state.json` contains the whitelisted ready/gesture/native-response reports, including `hasOpener` and `{mode,returnedNull}`. To request a fresh parent response after close, POST `{"target":"main"}` to `/command` with the fixture CA and inspect its `probe-<seq>` report. The same works for a child case ID. Preserve the receipt/logs/screenshots, then remove only the disposable fixture and its temporary CA trust.
