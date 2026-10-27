# Traffic: native packet capture for the Network module

> **Status (1.32.1): works end to end on Linux, measured with real packets. Not run on macOS or
> Windows, and not run yet with the permission grant on a real interface.** Everything below says
> which is which; read "What has and has not been verified" before relying on it elsewhere.

The corresponding SHVIA-WEB screen is the Network module's "Traffic" tool
(`docs/NETWORK/README.md` there). This document covers only the native half: what runs on the
user's own desktop machine, never on the server.

## Why a separate sidecar instead of linking libpcap into the app

Packet capture needs OS capability the rest of ShvIA Desktop never does (`CAP_NET_RAW` and
`CAP_NET_ADMIN` on Linux, read access to the BPF devices on macOS, the Npcap driver on Windows).
Granting that to the whole GUI process — the thing that also renders remote web content — would make
every other bug in the app a potential path to raw network access.

**Wireshark's own architecture draws this exact line**: its GUI runs unprivileged, and a separate,
small program (`dumpcap`) is the only thing ever granted capture capability. This repository follows
the same split:

- `src-tauri/traffic-capture/` is its own crate and binary, the **only** thing here that depends on
  `pcap`. `src-tauri` (the main app) never links libpcap.
- `src-tauri/src/code_bridge/traffic.rs` only resolves the sidecar, spawns it, reads its lines, and
  answers the bridge actions below. It never opens a device itself.
- The sidecar runs **only** while a capture runs: it starts on the person's explicit Start (after a
  native confirmation), and it lives exactly as long as its stdin stays open. Stop closes it; a dead
  or killed app closes it too. A crashed app never leaves a privileged capture running.

## Libraries (`traffic-capture/Cargo.toml`)

- **`pcap`**: bindings over libpcap (Linux/macOS) and Npcap's compatible API (Windows) — the engine
  Wireshark, `tshark`, `tcpdump` and `nmap` use. One dependency, one capture path on all three OSes.
- **`etherparse`**: Ethernet/VLAN/IPv4/IPv6/TCP/UDP/ICMP header parsing for the table. The tolerant
  ("lax") parser, because a NIC doing segmentation offload hands libpcap frames whose IPv4 length
  field is 0, which the strict parser refuses. No protocol-dissector framework: out of scope here.
- **`pcap-file`**: writes the **pcapng** container. Two facts about it cost a bug each and are now
  tests: it writes only the section header (the interface description block is the caller's job —
  without it the file is invalid and `pcap-file` itself refuses to write a packet), and it writes
  times in nanoseconds (the description must say so, or a reader assumes microseconds and every date
  is a thousand times too large).

## The sidecar's contract

```
traffic-capture --list-interfaces
traffic-capture --check-permission [--interface <id>]
traffic-capture --capture --interface <id> [--filter <bpf>] --out <path.pcapng> [--max-bytes <n>]
```

Every value comes from a web page, so parsing is strict: each flag takes exactly the next token (an
interface called `--out` stays an interface name), an unknown or repeated flag is an error, and the
interface must be one libpcap itself lists.

One JSON object per line on stdout:

| Line | Meaning |
|---|---|
| `{"interfaces":[{id,name,description,isUp,isLoopback}]}` | `--list-interfaces`. `name` is the id on Linux/macOS and the adapter's own description on Windows. Up interfaces first, loopback after. |
| `{"status":"ready"\|"needs_permission"\|"error",message?}` | `--check-permission` (opens a device briefly, reads nothing). |
| `{"status":"started","interface","linktype","snaplen"}` | The device is open, the filter compiled and the file created. The first line of a capture. |
| `{"packets":[{id,time,src,dst,protocol,length,summary,hex}],"total","notShown","flushed"}` | Rows for the table, at most ten lines a second. `time` is seconds since the first packet. `hex` is a Wireshark-style dump of the first 512 bytes (the note at its end says how many more the file holds). |
| `{"status":"stopped","reason":"requested"\|"size_limit"\|"error","total","bytes","kernelDropped"}` | The last line. `kernelDropped` is libpcap's own count of packets the kernel could not hand over. |
| `{"error","code"}` | The capture could not start or had to end. Codes: `E_CAPTURE_PERMISSION_DENIED`, `E_INTERFACE_UNAVAILABLE`, `E_CAPTURE_FILTER_INVALID`, `E_CAPTURE_WRITE`, `E_CAPTURE_FAILED`. |

How it keeps up and stays bounded:

- The capture file gets **every** packet (full length, snaplen 65,535). The table gets at most 100
  rows per tick; the rest are counted in `notShown`, so a port scan or a LAN transfer cannot flood the
  webview. A flood of 254,500 datagrams in 1.5 s was captured whole (254,503 packets in the file) with
  1,520 rows delivered and nothing dropped by the kernel.
- The file is created private (`0600`, never overwriting) and is flushed every tick, with `flushed`
  saying how many bytes end on a packet boundary — what Export may copy while the capture still runs.
- `--max-bytes` (default 512 MiB) ends the capture with `reason: size_limit`; the file stays exportable.
- The capture is **non-blocking** and sleeps 10 ms when idle. libpcap's own read timeout is not
  dependable: measured on this machine, `pcap_next_ex` on an idle interface did not return in 8 s with a
  100 ms timeout, so a blocking loop would never notice the stop request.
- Promiscuous mode is off: it sees what reaches **this computer's** interface, as the page says.

## The bridge actions (`window.__shviaCode`, flagged by `recursos.captura`)

| Action | What it does |
|---|---|
| `trafficInterfaces()` | The array above. Rejects `E_CAPTURE_UNAVAILABLE` when the build has no sidecar. |
| `trafficPermissionStatus({interfaceId?})` | `ready` / `needs_permission` / `missing_npcap` / `error`. The capability belongs to the binary, so the interface is optional. |
| `trafficRequestPermission()` | The one action that may ask the OS for elevation. Answers with the probe's result **after** the attempt, never with what the attempt hoped for, and a `reason` when nothing was granted (`cancelled`, `needs_chmodbpf` on macOS, `needs_npcap` on Windows) that the page turns into the next step for that OS. |
| `trafficStart({interfaceId, filter})` | A **native confirmation** naming the interface and the filter, then the sidecar; resolves `{started:true}` only once the device is open. A refusal (declined, no permission, bad filter, unknown interface, busy) rejects with `E_CODE: text` — the page never waits on a capture that never began. |
| `trafficStop()` | Closes the sidecar's stdin, waits up to 3 s for it to finish the file, kills it after that. The file stays for Export. |
| `trafficExport()` | A native save dialog, then a copy of the packets written so far — only whole packet blocks. Works while capturing and after Stop. |

Events through `onEvent`: `traffic_packet` (the rows line), `traffic_status` (`state: "stopped"` and
why, including `engine_exited` when the sidecar died without a closing line) and `traffic_error`.

The capture belongs to the page that started it: a new document in the window, a closed window or
the app quitting stops it and **deletes the file** (nobody is left to export it). Leftovers from a
crashed run are removed at the next start. The folder is the app's own cache, private to the person.
One capture at a time, system-wide (`E_CAPTURE_BUSY`).

## Permission model, per OS (never the whole app as root)

- **Linux**: `trafficRequestPermission()` runs **one** polkit prompt (`pkexec`) that, for the sidecar
  binary only, makes it `root:<your group>`, mode `0750`, and gives it `cap_net_raw,cap_net_admin`.
  The group and mode matter: file capabilities on a world-executable binary would let every local
  user capture. A reinstall replaces the file and the grant has to be made again; the page's permission
  note comes back by itself. The `.deb`/`.rpm` declare `libpcap0.8`/`libpcap` as a dependency. **The
  AppImage cannot be granted** (its files live on a read-only mount): use the `.deb`/`.rpm`.
- **macOS**: capture needs read access to `/dev/bpf*`, which a stock system reserves for root. The
  common fix is Wireshark's ChmodBPF helper. This app does **not** run an unverified privileged
  command to imitate it: `trafficRequestPermission()` says what to install and where (the plan this
  version replaces assumed an `access_bpf` group exists on every Mac — it does not; Wireshark's
  installer creates it).
- **Windows**: not bundled in this version. Building the crate needs the Npcap SDK and running it
  needs the Npcap driver; neither can be provided from here. A Windows build says capture is
  unavailable. (`missing_npcap` is reported by the code that will carry it.)

## Packaging

Linux and macOS declare the sidecar in `tauri.linux.conf.json` / `tauri.macos.conf.json` next to
`anna` and the Pentest engine; Windows (the base `tauri.conf.json`) declares only `anna`.
`scripts/stage-traffic-capture.mjs` (`npm run traffic-capture:stage`) builds it with `cargo build
--release` and **runs it once** (`--list-interfaces`) before staging, so a binary that cannot load
(libpcap missing on the build machine) is never bundled. `build-local.sh` stages it as step `[D7]`
(`--no-traffic` skips it) and builds the `externalBin` list from what is staged, so any one of the
three sidecars can be absent without dropping the others (`npm run prova:sem-anna` checks all eight
combinations). The build machine needs `libpcap-dev` on Linux; macOS has libpcap in the SDK.

## What has and has not been verified

**Measured on Linux, with real packets** (no sudo, no real interface: inside `unshare -Urn` the test
is root of a throw-away network namespace and owns `CAP_NET_RAW` over that namespace's own `lo` only):

- `scripts/traffic-e2e.sh` runs two tests (`code_bridge::traffic`, marked `#[ignore]`) through the
  **real** `start → handshake → rows → stop → export → release` code against the **real** sidecar: a bad
  filter and an unknown interface are refused at Start and leave no file; five UDP datagrams arrive as
  five rows with their payload in the hex; Stop keeps the file; the exported file has the section header,
  then the interface description, then five packet blocks, mode `0600`; replacing the page deletes it;
  killing the sidecar from outside tells the page (`engine_exited`) and keeps the file exportable.
- The exported files were read back with **libpcap** (a reader independent of the one that wrote
  them): same packet count, same bytes, the right times. A filter (`udp port 9999`) captured exactly
  the three matching packets; an invalid one (`tcp port (`) was refused with
  `E_CAPTURE_FILTER_INVALID`.
- `cargo test` in the sidecar crate (21 tests, including that pcapng round trip) and in the app,
  `cargo clippy --all-targets -- -D warnings` for both, and the app's clippy cross-compiled to
  `x86_64-pc-windows-gnu`.
- `cargo deny` on the sidecar's own lock, with the app's policy: advisories, bans, licenses and
  sources ok.

**Not verified:**

- **The grant on a real interface**: `pkexec` + `setcap` was not run on this machine (it needs a
  polkit prompt a person answers), and no capture was taken on a physical interface.
- **The whole thing inside the running app, in a window**: the native confirmation dialog and the
  page's rendering of the rows were not exercised together, because the app was not run. The two halves
  were checked against each other in pieces: SHVIA-WEB's Traffic page (branch `fix/traffic-desktop-contract`)
  was rendered in Chromium with this repository's **real bridge script** (`BRIDGE_JS` from `shim.rs`) and
  answered with the sidecar's **real output** — the real interface list, the real permission probe
  (`needs_permission` without the capability), rows with their hex from a real capture, the real refusal of
  an invalid filter, a flood, a capture ended by the engine, and the per-OS permission steps — and the
  native flow above ran against the real sidecar without a window.
- **macOS and Windows**: not built, not run. The macOS-only code (BSD loopback framing, the 32-bit
  `tv_usec`) is compiled by the new macOS CI job, which has not run since GitHub Actions is locked for
  this account; Windows has no sidecar in this version.
- **Packaging a real `.deb`/`.rpm`** with the sidecar inside, and installing it.

## Limitations (by design, for this version)

- No protocol dissection beyond Ethernet/VLAN/ARP/IPv4/IPv6/TCP/UDP/ICMP headers — no reassembly,
  no application-layer decoding. The table shows the first 512 bytes of each packet in hex; the rest
  is in the exported file.
- No TLS decryption of any kind.
- Only traffic that reaches the chosen interface — this is not "see the whole network", and nothing
  here promises otherwise (promiscuous mode is off, and a switch only sends this computer its own).
- One capture at a time, one interface at a time.
- The capture filter (BPF, applied by libpcap before packets reach the app) and the page's display
  filter (client-side, over the rows already received) are different things; SHVIA-WEB's page says so.
