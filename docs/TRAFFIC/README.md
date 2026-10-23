# Traffic: native packet capture for the Network module

> **Status at landing: Linux build-and-unit-test verified. Real capture (opening a device
> with elevated capability and reading live frames) was NOT exercised on this machine by
> design** — the session that wrote this could not install `libpcap-dev` or grant any OS
> capability without a human's own explicit action, so that step is still pending. macOS and
> Windows are code-complete for the same reason `pcap` was chosen (one capture code path for
> all three OSes) but have never been compiled or run — there is no such hardware here. See
> "What has and has not been verified" below before relying on this outside Linux.

The corresponding SHVIA-WEB screen is the Network module's "Traffic" tool
(`docs/NETWORK/README.md` there). This document covers only the native half: what runs on
the user's own desktop machine, never on the server.

## Why a separate sidecar instead of linking libpcap into the app

Packet capture needs OS capability the rest of ShvIA Desktop never does
(`CAP_NET_RAW`/`CAP_NET_ADMIN` on Linux, BPF device group access on macOS, the Npcap driver
on Windows). Granting that to the whole GUI process — the thing that also renders remote web
content — would make every other bug in the app a potential path to raw network access.

**Wireshark's own architecture draws this exact line**: its GUI runs unprivileged, and a
separate, small program (`dumpcap`) is the only thing ever granted capture capability. This
repository follows the same split:

- `src-tauri/traffic-capture/` is its own crate, its own binary, the **only** thing in this
  repository that depends on `pcap` (libpcap/Npcap bindings). `src-tauri` (the main app) does
  not depend on it and never links libpcap.
- `src-tauri/src/code_bridge/traffic.rs` only resolves the sidecar's path, spawns it, reads
  its stdout, and answers the bridge actions below. It never opens a device itself.
- The sidecar is spawned **only** while a capture is running (the page's explicit "Start"
  click) and is killed on Stop or app exit — never at app start, never implicitly.

## Library choices (in `traffic-capture/Cargo.toml`)

- **`pcap`**: Rust bindings over libpcap (Linux/macOS) and Npcap's WinPcap-compatible API
  (Windows) — the same underlying engine Wireshark, `tshark`, `tcpdump` and `nmap` use. One
  dependency, one code path, correct behavior on all three target OSes.
- **`etherparse`**: lightweight Ethernet/ARP/IPv4/IPv6/TCP/UDP/ICMP header parsing for the
  table's source/destination/protocol/length/summary columns. No full protocol-dissector
  framework — out of scope for this first version.
- **`pcap-file`**: writes spec-correct **PCAPNG** so an exported capture opens cleanly in
  Wireshark without this repository hand-rolling the binary container format.

## The sidecar's command surface

`traffic-capture` (built from `src-tauri/traffic-capture/`) takes one of three forms, each
printing NDJSON (one JSON object per line) to stdout:

```
traffic-capture --list-interfaces
traffic-capture --check-permission --interface <id>
traffic-capture --capture --interface <id> [--filter <bpf-expression>] --out <path.pcapng>
```

`--list-interfaces` and `--check-permission` need no elevated capability on any of the three
OSes (device *enumeration* and a brief open-then-close probe both work unprivileged); only
`--capture` needs the real permission described below. `--check-permission`'s point is to let
the page ask "would Start work?" without ever starting a real capture.

## The bridge actions (`window.__shviaCode`, flagged by `recursos.captura`)

| Action | What it does |
|---|---|
| `trafficInterfaces()` | Local interfaces, via `--list-interfaces`. |
| `trafficPermissionStatus(interfaceId)` | `ready` / `needs_permission` / `missing_npcap` (Windows only) / `unsupported_platform` / `error`. |
| `trafficRequestPermission()` | The ONE explicit, user-initiated action that may trigger an OS elevation prompt (see below). Never run on its own; no "always allow". |
| `trafficStart({interfaceId, filter})` | Spawns the sidecar with `--capture`; frames stream back as `_emit` events (`traffic_packets` batches, `traffic_error`, `traffic_status`), the same push channel agent sessions already use — not polling. |
| `trafficStop()` | Kills the sidecar. Also called with no window (`None`) on app exit (`RunEvent::Exit`, `lib.rs`), the same lifecycle `pentest::stop` already has — closing the LAST window to the tray does **not** stop a running capture, matching how a Code Mode session also survives that; only an explicit Stop or a real app exit does. |
| `trafficExport()` | Copies the session's temp `.pcapng` file to wherever the user picks in a native save dialog (`tauri_plugin_dialog`) — a plain file copy, not a round trip through the JS bridge, so export size is not bounded by the existing `saveFile` action's 50 MB attachment cap. |

Only one capture runs at a time system-wide (`trafficStart` refuses a second one while one is
active) — a deliberate v1 simplification, not a resource limit worth working around yet.

## Permission model, per OS (no admin/root for the whole app)

- **Linux**: packaged (deb/rpm) builds run `setcap cap_net_raw,cap_net_admin+eip` on the
  `traffic-capture` binary **only**, in the package's post-install step (exactly how
  Debian/Fedora package Wireshark's `dumpcap`) — **this step is written into the plan but not
  yet wired into the packaging scripts in this delivery; see "Not done here" below.** For a
  portable/AppImage build, or before that packaging step exists, `trafficRequestPermission()`
  runs `pkexec setcap cap_net_raw,cap_net_admin+eip <path-to-sidecar>` — one explicit prompt,
  for that one binary, triggered only by the user's own "Enable capture" click.
- **macOS**: `trafficRequestPermission()` runs `osascript ... with administrator privileges`
  to add the current user to the `access_bpf` group (built in since Ventura) — one-time,
  standard for any capture tool, the same effect as Wireshark's own `ChmodBPF` helper on
  older macOS.
- **Windows**: requires **Npcap** installed separately (the same real-world prerequisite
  Wireshark and Nmap both carry). `trafficPermissionStatus` reports `missing_npcap` with a
  link to the official installer; ShvIA itself never requests elevation on Windows.

None of this was executed against a real OS permission boundary in this delivery — see below.

## What has and has not been verified

**Verified on this machine (Linux):**
- `cargo check` / `cargo test` / `cargo clippy --all-targets -- -D warnings` for the main
  `src-tauri` crate, including the new `code_bridge::traffic` module and its unit tests
  (resolver, the stop-with-nothing-running path, the capture-filter/argv boundary).
- The bundling wiring: `tauri.conf.json`'s new `externalBin` entry, and that `cargo check`
  fails without a staged `traffic-capture-<triple>` stand-in and succeeds with one, the same
  contract the existing `anna` sidecar already has.

**Not verified, and why:**
- **The `traffic-capture` crate itself was never compiled.** It depends on `pcap`, which
  needs the `libpcap-dev` system package at build time; installing it requires `sudo` on this
  shared machine, which this session deliberately did not do (see "Permission model" above —
  the same posture applies to the session's own actions, not only to the shipped product).
  Its `main.rs`/`parse.rs` were written from general knowledge of the `pcap`/`pcap-file`/
  `etherparse` APIs, not against the exact resolved crate versions — **a build-fix pass is a
  real, expected next step**, not a formality. `args.rs` and `parse.rs`'s own logic (pure
  Rust, no `pcap` types) are unit-tested and read correctly, but the whole crate could not be
  type-checked as a unit.
- **No real device was opened, on any OS.** `--check-permission`'s and `--capture`'s actual
  behavior against a live interface, a real "permission denied", a real Npcap absence, and
  the exported file's validity in Wireshark are all unverified.
- **`trafficRequestPermission()`'s `pkexec`/`osascript` commands were never run.** They are
  real, reachable code — not stubs — but exercising them means asking this shared machine for
  a capability no other session should silently also end up with.
- **macOS and Windows were not built at all** (no such hardware in this session). The `pcap`
  crate's own cross-platform claim is the basis for believing the Linux-verified code path
  also makes sense there, not a build that ran.

## Limitations (by design, for this first version)

- No protocol dissection beyond Ethernet/ARP/IPv4/IPv6/TCP/UDP/ICMP headers — no reassembly,
  no application-layer decoding.
- No TLS decryption of any kind.
- Only traffic reachable to the chosen interface — this is not "see the whole network", and
  nothing here promises otherwise.
- One capture at a time, one interface at a time.
- The capture filter (BPF, applied natively, kernel-level) and the page's own display filter
  (client-side, over already-captured rows) are deliberately different things — see the
  SHVIA-WEB side's docs.

## Not done here (known follow-ups)

- The Linux deb/rpm post-install `setcap` packaging step described above is documented but
  not yet written into this repository's packaging scripts.
- `build-local.sh`'s existing `--no-anna`/"no anna found" fallback overrides the ENTIRE
  `bundle.externalBin` to `[]` when `anna` is missing (`build-local.sh:1515-1521`). Until that
  script learns to compose per-sidecar overrides instead of one all-or-nothing switch, a local
  build made without `anna` staged also ships without `traffic-capture`, even if the latter
  **was** staged (`npm run traffic-capture:stage`). Building with both staged bundles both
  correctly; this is only a gap in the "ship partial" paths.
- Real-hardware acceptance (a live capture, a real permission prompt, a `.pcapng` opened in
  Wireshark) on Linux, and any build at all on macOS/Windows, as listed above.
