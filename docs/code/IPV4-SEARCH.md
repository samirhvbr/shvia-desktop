# IPv4 Search — find what answers on a private range, from this computer

> **Status:** shipped in 1.31.0 (`recursos.varreduraIPv4`). The page is SHVIA-WEB's `/network/ipv4-search`
> (`docs/NETWORK/README.md` there); this document is the native half. Decision record: [ADR-044](../decisoes.md).

The person types a range (`10.100.100.0/22`) and, optionally, ports (`2000,8000-8080`). The shell tries to open a TCP
connection to each port of each address, closes it at once, and reports which addresses answered and what each port did.
The motivating question was "which IP has port 8006 open?" — Proxmox's web UI — on a /22 the owner administers.

## What it does, exactly

For every probe it makes one TCP `connect()` with a time limit and drops the connection when it is accepted. **No byte is
ever sent**: no banner grab, no payload, no protocol conversation. The answer is one of:

| Answer | What happened | Meaning |
|---|---|---|
| `open` | the connection was accepted | something listens |
| `closed` | the host refused it (RST) | the host is alive, nothing listens |
| `filtered` | no answer inside the time limit | a firewall dropped it, or nothing is there |
| unreachable | the network said "no route" | the address is not there; its other probes are skipped |
| error | the probe could not be made (out of descriptors, odd OS error) | counted apart, **never** read as `filtered` |

On Windows a refused connection is not reported at once: the stack answers a reset by sending the SYN again, twice, and fails
the connect about two seconds later, which would turn every `closed` into `filtered` inside the default time limit. The probe
creates its socket itself and sets `SIO_TCP_INITIAL_RTO` to "no SYN retransmissions" before connecting, so the reset is
final. It is best effort: on a Windows that ignores the option (older than 8) `closed` can still read as `filtered` unless
the time limit is raised past about two seconds.

An address is **found** when any probe came back `open` or `closed`. One that only ever times out is "no answer": a
connect scan cannot tell a dead address from a host that silently drops everything, and the report says so instead of
guessing.

### Two passes

Trying every port of every address is slow when most addresses are empty, so the scan works in two passes, like `nmap`
without `-Pn`:

1. **Pass 1** tries a small set on every address: the first four of the person's own ports, then the common ones
   (`443, 80, 22, 445, 3389, 8006, 139, 135`), eight in all. With no ports given this is the whole scan ("discovery").
2. **Pass 2** tries the person's remaining ports, only on the addresses that answered in pass 1.

`probeAll: true` skips the shortcut and tries every port on every address — slower, and the right choice for a host that
drops the common ports yet listens on an odd one. The report speaks only of the ports the person asked for; the common
ports of pass 1 are a means of finding the host, not an answer (with no ports asked, they ARE the answer).

Probes go port-major (port 1 across the whole range, then port 2), so a single host is never hit by many probes in a row.
Up to 128 run at once; the shell raises the process's descriptor limit when it can, because a macOS GUI app starts with a
soft limit of 256 shared with the webview, and falls back to 64 or 32 when it cannot.

## The limits live here, not in the page

The page validates for a good message; **this module decides** (`code_bridge/ipv4_search.rs`):

- **Private ranges only**: RFC1918, CGNAT (the Tailscale range) and `198.18.0.0/15`, the same classes as the WEB's
  `DestinationPolicy`. A public range is **refused**, not offered for confirmation (`E_SCAN_PUBLIC_RANGE`); loopback,
  link-local, multicast and reserved space are never a target (`E_SCAN_FORBIDDEN_RANGE`). The tool finds what is on the
  person's own network; it is not a way to look at somebody else's.
- **At most a /16** (65,536 addresses), **at most 1,024 distinct ports**, and at most 1,000,000 pass-1 probes
  (`addresses × pass-1 ports`). A hard ceiling of 5,000,000 probes stops a scan on its own — pass 2 is bounded by the hosts
  that answered, which is small, except behind a middlebox that answers for every address.
- **Time limit per connection** 200–5,000 ms, default 800 ms (`timeoutMs`).
- **One scan at a time**, app-wide. The slot is taken *before* the dialog, so a second request cannot open a second dialog
  behind the first (`E_SCAN_BUSY`).
- **Cloud metadata addresses inside an allowed range are left out** (`100.100.100.200`, Alibaba's, sits inside the CGNAT
  range) instead of refusing the whole range; the report counts them as `skipped`.
- A **native confirmation dialog** shows the range as normalised (`10.100.100.7/22` becomes `10.100.100.0/22`), the number of
  addresses, the ports, the time limit and the most attempts it can make. When the person typed ports, it also names the
  ports pass 1 tries on every address (the first four typed, then the common ones) and the ones left for pass 2, because
  the dialog is what the person consents to. The page cannot answer it. Declining ends in
  `E_SCAN_DECLINED`. If the window starts a new document while the dialog is open, the answer starts nothing and the
  request ends in `E_SCAN_WINDOW_CHANGED`, as it does when the window closes.
- The scan **stops** on `ipv4SearchStop` from the window that started it (from any other window the call answers
  `{ stopped: false }` and changes nothing), when the window starts a new document, when the window closes and when the app
  quits. Nothing takes a running scan back after a reload, so a result nobody can read is never produced.

The whole-range zone check looks at the first and last address only. That is sound because every accepted range is a /16
or narrower and every zone boundary is a /16 or wider; a test proves it over all 65,536 possible /16 blocks.

## Bridge contract

`window.__shviaCode.recursos.varreduraIPv4 === true` is the presence flag (never a version number).

```js
ipv4SearchStart({ target: '10.100.100.0/22', ports: '2000,8000-8080', timeoutMs: 800, probeAll: false })
  // → Promise<{ scanId, cidr, addresses, ports, discoveryPorts }>, AFTER the person confirms
  // → rejects with a string 'E_SCAN_*: text'
ipv4SearchStop() // → Promise<{ stopped: boolean }>
```

Events arrive through `onEvent`, all carrying `scanId`:

| `type` | Fields | When |
|---|---|---|
| `ipv4_search_progress` | `pass` (1 or 2), `done`, `total`, `found` | about every 200 ms |
| `ipv4_search_host` | `ip` | the first time an address answers |
| `ipv4_search_done` | `stopped`, `capped`, `cidr`, `addresses`, `skipped`, `ports`, `discoveryPorts`, `portCount`, `found`, `probes`, `errors`, `durationMs`, `hosts[]` | once, at the end — also after a stop |

Each entry of `hosts` is `{ ip, open: [22, 8006], closed: "80,443", filtered: "8000-8080" }`: `open` is a list of numbers,
`closed` and `filtered` are compact range strings, because a /22 scanned over 80 ports has a lot of them and every byte
goes through `eval`.

Refusal codes: `E_SCAN_INVALID_TARGET`, `E_SCAN_PUBLIC_RANGE`, `E_SCAN_FORBIDDEN_RANGE`, `E_SCAN_TOO_LARGE`,
`E_SCAN_INVALID_PORTS`, `E_SCAN_TOO_MANY_PORTS`, `E_SCAN_TOO_MANY_PROBES`, `E_SCAN_INVALID_OPTIONS`,
`E_SCAN_NOTHING_TO_PROBE`, `E_SCAN_BUSY`, `E_SCAN_DECLINED`, `E_SCAN_WINDOW_CHANGED`. The page translates a code it knows
and never renders the free text after the colon.

## How it was proved, and what was not

Proved by the unit tests in `ipv4_search.rs` (33): the target and port parsers over malformed input, the range policy
(every private class accepted, public and forbidden refused, the /16 invariant), the two passes on a scripted network
(who is found, that pass 2 never touches a silent host, that `probeAll` reaches a host the shortcut would miss, that an
unreachable address stops being probed, that an error is not counted as `filtered`), cancellation, the events, the
report, the one-slot rule — and the real TCP probe against listening and non-listening sockets on loopback. The target
and port parsers are also tested against `ipv4_search_vectors.json`, a copy of the vectors SHVIA-WEB's page parser is tested
against (`docs/NETWORK/ipv4-search-vectors.json` there): 72 inputs with their expected result, so the page and this module
cannot drift apart without a visible edit in both repositories. Nothing compares the two files with each other.

**Not measured:** a scan across a real network with hosts that drop packets (the timeout path is a real
`connect_timeout`, but no test waits on one), the Windows option on a real remote host (the CI job proves it on loopback only: a refused port must come back `closed` inside 500 ms), and how long a /22 takes on the owner's network.
