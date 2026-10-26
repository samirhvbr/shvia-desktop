//! IPv4 Search (1.33.0, `recursos.varreduraIPv4`): a TCP connect scan of ONE private IPv4 range, run from
//! THIS computer. The server never sees the range, the ports or the result — the page only asks, and this
//! module decides, confirms with the person and scans.
//!
//! ## What it does and does not do
//!
//! For every address it tries to open a TCP connection to the ports asked for and drops it at once: no byte is
//! ever sent, so there is no banner grab, no payload and no exploit. What comes back is one of three answers —
//! `open` (the connection was accepted), `closed` (the host refused it: it is alive and nothing listens) or
//! `filtered` (no answer inside the time limit: a firewall, or nothing there). A host that answers `open` or
//! `closed` on any probe is "found"; one that only ever times out is "no answer", because a connect scan cannot
//! tell a dead address from a host that silently drops everything.
//!
//! ## Two passes, like `nmap` without `-Pn`
//!
//! Probing every port of every address is slow when most addresses are empty. So pass 1 tries a small set on
//! every address (the first few ports the person asked for plus the common ones) and pass 2 tries the remaining
//! ports only on the addresses that answered. `probeAll` skips the shortcut for a host that drops the common
//! ports yet listens on an odd one — slower, and the person chooses it.
//!
//! ## The limits are native, because the page cannot be the guard
//!
//! The page validates for a good message; THIS file decides. A range must be wholly private (RFC1918, CGNAT,
//! benchmark space — the same classes as `App\Services\Pentest\DestinationPolicy` on the WEB side; the IPv6 half
//! is absent because this tool is IPv4-only), at most a /16, at most 1,024 ports, and the person confirms in a
//! native dialog that shows what is about to happen. A public range is refused, not confirmed: this tool finds
//! what is on the person's own network, it is not a way to look at somebody else's.

use super::*;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The widest range accepted: a /16, 65,536 addresses.
pub(crate) const MIN_PREFIX: u8 = 16;
/// Most distinct ports in one scan.
pub(crate) const MAX_PORTS: usize = 1024;
/// Most connection attempts pass 1 may need (`addresses × pass-1 ports`). Refused up front, with a clear reason.
pub(crate) const MAX_FIRST_PASS_PROBES: u64 = 1_000_000;
/// A hard ceiling on everything a scan tries. Pass 2 is bounded by the hosts that answered, which is small in
/// practice — except behind a middlebox that answers for every address, which this stops.
const MAX_TOTAL_PROBES: u64 = 5_000_000;
/// Ports tried on every address when the person gave none, and the tail of the pass-1 set when they did.
const DISCOVERY_PORTS: [u16; 8] = [443, 80, 22, 445, 3389, 8006, 139, 135];
/// How many of the person's own ports lead the pass-1 set.
const DISCOVERY_OWN: usize = 4;
const DISCOVERY_MAX: usize = 8;
const DEFAULT_TIMEOUT_MS: u64 = 800;
const MIN_TIMEOUT_MS: u64 = 200;
const MAX_TIMEOUT_MS: u64 = 5_000;

// ── refusals ───────────────────────────────────────────────────────────────────────────────────

/// Why a scan was not started. The page reads the stable `E_SCAN_*` code and writes its own sentence; the text after
/// the colon is for a log, never rendered (the same convention as `E_PENTEST_*` and `E_CAPTURE_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    InvalidTarget,
    PublicRange,
    ForbiddenRange,
    TooLarge,
    InvalidPorts,
    TooManyPorts,
    TooManyProbes,
    InvalidOptions,
    NothingToProbe,
    Busy,
    Declined,
    WindowChanged,
}

impl Refusal {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Refusal::InvalidTarget => "E_SCAN_INVALID_TARGET",
            Refusal::PublicRange => "E_SCAN_PUBLIC_RANGE",
            Refusal::ForbiddenRange => "E_SCAN_FORBIDDEN_RANGE",
            Refusal::TooLarge => "E_SCAN_TOO_LARGE",
            Refusal::InvalidPorts => "E_SCAN_INVALID_PORTS",
            Refusal::TooManyPorts => "E_SCAN_TOO_MANY_PORTS",
            Refusal::TooManyProbes => "E_SCAN_TOO_MANY_PROBES",
            Refusal::InvalidOptions => "E_SCAN_INVALID_OPTIONS",
            Refusal::NothingToProbe => "E_SCAN_NOTHING_TO_PROBE",
            Refusal::Busy => "E_SCAN_BUSY",
            Refusal::Declined => "E_SCAN_DECLINED",
            Refusal::WindowChanged => "E_SCAN_WINDOW_CHANGED",
        }
    }

    fn text(self) -> &'static str {
        match self {
            Refusal::InvalidTarget => "O alvo não é um IPv4 ou uma faixa CIDR válida (ex.: 10.100.100.0/22).",
            Refusal::PublicRange => "Só faixas de rede privada podem ser varridas (10.x, 172.16–31.x, 192.168.x, 100.64–127.x).",
            Refusal::ForbiddenRange => "Esta faixa (loopback, link-local, multicast ou reservada) nunca é um alvo de varredura.",
            Refusal::TooLarge => "A faixa é grande demais: o máximo é uma /16 (65.536 endereços).",
            Refusal::InvalidPorts => "As portas não estão no formato esperado (ex.: 2000,8000-8080), de 1 a 65535.",
            Refusal::TooManyPorts => "Há portas demais: o máximo é 1.024 por varredura.",
            Refusal::TooManyProbes => "A combinação de faixa e portas gera tentativas demais; reduza a faixa ou as portas.",
            Refusal::InvalidOptions => "Uma das opções da varredura está fora do intervalo permitido.",
            Refusal::NothingToProbe => "Para sondar todos os endereços é preciso informar ao menos uma porta.",
            Refusal::Busy => "Já há uma varredura em andamento ou aguardando confirmação.",
            Refusal::Declined => "A varredura não foi autorizada.",
            Refusal::WindowChanged => "A janela mudou durante a confirmação.",
        }
    }

    pub(crate) fn message(self) -> String {
        format!("{}: {}", self.code(), self.text())
    }
}

// ── the range ──────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Zone {
    Private,
    Never,
    Public,
}

/// Addresses that are never a scan target. Mirrors `DestinationPolicy::NEVER`, IPv4 only.
const NEVER_RANGES: [([u8; 4], u8); 5] = [
    ([0, 0, 0, 0], 8),     // this host
    ([127, 0, 0, 0], 8),   // loopback
    ([169, 254, 0, 0], 16), // link-local, where cloud metadata lives
    ([224, 0, 0, 0], 4),   // multicast
    ([240, 0, 0, 0], 4),   // reserved, includes the broadcast address
];

/// The ranges a scan may cover. Mirrors `DestinationPolicy::PRIVATE`, IPv4 only.
const PRIVATE_RANGES: [([u8; 4], u8); 5] = [
    ([10, 0, 0, 0], 8),
    ([172, 16, 0, 0], 12),
    ([192, 168, 0, 0], 16),
    ([100, 64, 0, 0], 10), // CGNAT — the Tailscale range, which is the owner's own network
    ([198, 18, 0, 0], 15), // benchmark space: a lab
];

/// Cloud metadata endpoints. Two of them sit INSIDE a range a scan is allowed to cover, so they are left out of the
/// targets instead of refusing the whole range.
const METADATA: [[u8; 4]; 2] = [[100, 100, 100, 200], [192, 0, 0, 192]];

fn in_range(ip: Ipv4Addr, base: [u8; 4], prefix: u8) -> bool {
    let mask = if prefix == 0 { 0 } else { u32::MAX << (32 - u32::from(prefix)) };
    (u32::from(ip) & mask) == (u32::from(Ipv4Addr::from(base)) & mask)
}

pub(crate) fn zone(ip: Ipv4Addr) -> Zone {
    if NEVER_RANGES.iter().any(|&(base, prefix)| in_range(ip, base, prefix)) {
        return Zone::Never;
    }
    if PRIVATE_RANGES.iter().any(|&(base, prefix)| in_range(ip, base, prefix)) {
        return Zone::Private;
    }
    Zone::Public
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cidr {
    network: u32,
    prefix: u8,
}

impl Cidr {
    /// A bare address is a /32. Host bits are cleared (`10.100.100.7/22` is `10.100.100.0/22`) and the dialog shows
    /// the range as normalised, so the person confirms what will really be scanned.
    pub(crate) fn parse(text: &str) -> Result<Cidr, Refusal> {
        let text = text.trim();
        if text.is_empty() || text.len() > 18 {
            return Err(Refusal::InvalidTarget);
        }
        let (address, length) = text.split_once('/').unwrap_or((text, "32"));
        // `Ipv4Addr::from_str` already refuses leading zeros, which some stacks read as octal.
        let ip: Ipv4Addr = address.parse().map_err(|_| Refusal::InvalidTarget)?;
        if length.is_empty() || length.len() > 2 || !length.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Refusal::InvalidTarget);
        }
        let prefix: u8 = length.parse().map_err(|_| Refusal::InvalidTarget)?;
        if prefix > 32 {
            return Err(Refusal::InvalidTarget);
        }
        if prefix < MIN_PREFIX {
            return Err(Refusal::TooLarge);
        }
        Ok(Cidr { network: u32::from(ip) & Self::mask(prefix), prefix })
    }

    fn mask(prefix: u8) -> u32 {
        if prefix == 0 {
            0
        } else {
            u32::MAX << (32 - u32::from(prefix))
        }
    }

    pub(crate) fn first(&self) -> Ipv4Addr {
        Ipv4Addr::from(self.network)
    }

    pub(crate) fn last(&self) -> Ipv4Addr {
        Ipv4Addr::from(self.network | !Self::mask(self.prefix))
    }

    pub(crate) fn label(&self) -> String {
        format!("{}/{}", self.first(), self.prefix)
    }

    /// Every usable address: a /31 and a /32 have no network or broadcast address to leave out.
    fn hosts(&self) -> Vec<Ipv4Addr> {
        let (from, to) = if self.prefix >= 31 {
            (u64::from(self.network), u64::from(u32::from(self.last())))
        } else {
            (u64::from(self.network) + 1, u64::from(u32::from(self.last())) - 1)
        };
        (from..=to).map(|n| Ipv4Addr::from(n as u32)).collect()
    }
}

// ── the ports ──────────────────────────────────────────────────────────────────────────────────

/// `2000,8000-8080` → every port in it, sorted and without repeats. Empty text is fine (ports are optional).
/// Anything that is not a port or a plain ascending range is refused rather than guessed at.
pub(crate) fn parse_ports(text: &str) -> Result<Vec<u16>, Refusal> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let mut ports = std::collections::BTreeSet::new();
    for token in text.split(',') {
        let token = token.trim();
        let (low, high) = match token.split_once('-') {
            Some((a, b)) => (port_number(a)?, port_number(b)?),
            None => {
                let p = port_number(token)?;
                (p, p)
            }
        };
        if low > high {
            return Err(Refusal::InvalidPorts);
        }
        // Counted before it is expanded: `1-65535` must be refused without building 65,535 entries.
        if (u32::from(high) - u32::from(low) + 1) as usize > MAX_PORTS {
            return Err(Refusal::TooManyPorts);
        }
        for p in low..=high {
            ports.insert(p);
            if ports.len() > MAX_PORTS {
                return Err(Refusal::TooManyPorts);
            }
        }
    }
    Ok(ports.into_iter().collect())
}

fn port_number(text: &str) -> Result<u16, Refusal> {
    let text = text.trim();
    if text.is_empty() || text.len() > 5 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Refusal::InvalidPorts);
    }
    match text.parse::<u32>() {
        Ok(n) if (1..=65535).contains(&n) => Ok(n as u16),
        _ => Err(Refusal::InvalidPorts),
    }
}

/// `[80, 8000, 8001, 8002]` → `80,8000-8002`: what the report and the dialog print, and how closed/filtered ports
/// travel to the page (a /22 with 80 ports has a lot of them).
pub(crate) fn compact_ports(ports: &[u16]) -> String {
    let mut sorted = ports.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < sorted.len() {
        let mut j = i;
        while j + 1 < sorted.len() && u32::from(sorted[j + 1]) == u32::from(sorted[j]) + 1 {
            j += 1;
        }
        out.push(if j == i { sorted[i].to_string() } else { format!("{}-{}", sorted[i], sorted[j]) });
        i = j + 1;
    }
    out.join(",")
}

// ── the plan ───────────────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub(crate) struct Plan {
    pub(crate) cidr: Cidr,
    pub(crate) hosts: Vec<Ipv4Addr>,
    /// Addresses left out on purpose (cloud metadata endpoints inside the range).
    pub(crate) skipped: usize,
    /// What the person asked for, sorted. Empty = discovery only.
    pub(crate) ports: Vec<u16>,
    /// Tried on every address.
    pub(crate) first_pass: Vec<u16>,
    /// Tried only on the addresses that answered the first pass.
    pub(crate) second_pass: Vec<u16>,
    pub(crate) timeout: Duration,
}

impl Plan {
    pub(crate) fn first_pass_probes(&self) -> u64 {
        self.hosts.len() as u64 * self.first_pass.len() as u64
    }
}

fn discovery_set(own: &[u16]) -> Vec<u16> {
    let mut set: Vec<u16> = Vec::with_capacity(DISCOVERY_MAX);
    for &p in own.iter().take(DISCOVERY_OWN).chain(DISCOVERY_PORTS.iter()) {
        if set.len() == DISCOVERY_MAX {
            break;
        }
        if !set.contains(&p) {
            set.push(p);
        }
    }
    set
}

pub(crate) fn plan(target: &str, ports: &str, timeout_ms: Option<u64>, probe_all: bool) -> Result<Plan, Refusal> {
    let cidr = Cidr::parse(target)?;
    match (zone(cidr.first()), zone(cidr.last())) {
        (Zone::Private, Zone::Private) => {}
        (Zone::Never, _) | (_, Zone::Never) => return Err(Refusal::ForbiddenRange),
        _ => return Err(Refusal::PublicRange),
    }
    let ports = parse_ports(ports)?;
    if probe_all && ports.is_empty() {
        return Err(Refusal::NothingToProbe);
    }
    let timeout_ms = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    if !(MIN_TIMEOUT_MS..=MAX_TIMEOUT_MS).contains(&timeout_ms) {
        return Err(Refusal::InvalidOptions);
    }
    let all = cidr.hosts();
    let total = all.len();
    let hosts: Vec<Ipv4Addr> = all.into_iter().filter(|ip| !METADATA.contains(&ip.octets())).collect();
    let skipped = total - hosts.len();
    let (first_pass, second_pass) = if probe_all {
        (ports.clone(), Vec::new())
    } else if ports.is_empty() {
        (DISCOVERY_PORTS.to_vec(), Vec::new())
    } else {
        let first = discovery_set(&ports);
        let rest = ports.iter().copied().filter(|p| !first.contains(p)).collect();
        (first, rest)
    };
    let plan = Plan { cidr, hosts, skipped, ports, first_pass, second_pass, timeout: Duration::from_millis(timeout_ms) };
    if plan.first_pass_probes() > MAX_FIRST_PASS_PROBES {
        return Err(Refusal::TooManyProbes);
    }
    Ok(plan)
}

// ── the engine ─────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Probe {
    Open,
    Closed,
    Filtered,
    /// The network said there is no route to that address: it is not there, and its other probes are skipped.
    Unreachable,
    /// The probe could not be made (out of descriptors, an unexpected OS error). Never read as "filtered".
    Error,
}

/// `None` means the process is out of file descriptors: that says nothing about the target, so the caller backs off
/// and tries again instead of recording a false "filtered".
fn classify_error(e: &std::io::Error) -> Option<Probe> {
    use std::io::ErrorKind::*;
    #[cfg(unix)]
    match e.raw_os_error() {
        Some(n) if n == libc::EMFILE || n == libc::ENFILE => return None,
        Some(n) if n == libc::EHOSTDOWN => return Some(Probe::Unreachable),
        _ => {}
    }
    #[cfg(windows)]
    if e.raw_os_error() == Some(10024) {
        return None; // WSAEMFILE
    }
    Some(match e.kind() {
        ConnectionRefused | ConnectionReset | ConnectionAborted => Probe::Closed,
        TimedOut | WouldBlock => Probe::Filtered,
        HostUnreachable | NetworkUnreachable => Probe::Unreachable,
        _ => Probe::Error,
    })
}

/// One connection attempt, dropped the moment it is accepted. Sends nothing.
pub(crate) fn probe_tcp(ip: Ipv4Addr, port: u16, timeout: Duration) -> Probe {
    let address = SocketAddr::V4(SocketAddrV4::new(ip, port));
    for attempt in 0..3u64 {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(stream) => {
                drop(stream);
                return Probe::Open;
            }
            Err(e) => match classify_error(&e) {
                Some(answer) => return answer,
                None => std::thread::sleep(Duration::from_millis(40 * (attempt + 1))),
            },
        }
    }
    Probe::Error
}

pub(crate) enum Event {
    Found(Ipv4Addr),
    Progress { pass: u8, done: u64, total: u64, found: usize },
}

#[derive(Default)]
struct HostState {
    answered: bool,
    unreachable: bool,
    open: Vec<u16>,
    closed: Vec<u16>,
    filtered: Vec<u16>,
}

#[derive(Debug, Default)]
pub(crate) struct Found {
    pub(crate) ip: Option<Ipv4Addr>,
    pub(crate) open: Vec<u16>,
    pub(crate) closed: Vec<u16>,
    pub(crate) filtered: Vec<u16>,
}

#[derive(Debug, Default)]
pub(crate) struct Report {
    pub(crate) found: Vec<Found>,
    pub(crate) probes: u64,
    pub(crate) errors: u64,
    pub(crate) stopped: bool,
    /// The scan hit `MAX_TOTAL_PROBES` and stopped itself.
    pub(crate) capped: bool,
}

struct Shared<'a> {
    plan: &'a Plan,
    states: Vec<std::sync::Mutex<HostState>>,
    cancel: &'a AtomicBool,
    capped: AtomicBool,
    prober: &'a (dyn Fn(Ipv4Addr, u16, Duration) -> Probe + Sync),
    notify: &'a (dyn Fn(Event) + Sync),
    probes: AtomicU64,
    errors: AtomicU64,
    found: std::sync::atomic::AtomicUsize,
}

impl Shared<'_> {
    fn halted(&self) -> bool {
        self.cancel.load(Ordering::Relaxed) || self.capped.load(Ordering::Relaxed)
    }

    fn record(&self, host: usize, port: u16, answer: Probe) {
        let mut announce = false;
        {
            let mut state = self.states[host].lock().unwrap_or_else(|e| e.into_inner());
            match answer {
                Probe::Open | Probe::Closed => {
                    if answer == Probe::Open {
                        state.open.push(port);
                    } else {
                        state.closed.push(port);
                    }
                    if !state.answered {
                        state.answered = true;
                        announce = true;
                    }
                }
                Probe::Filtered => state.filtered.push(port),
                Probe::Unreachable => {
                    if state.answered {
                        state.filtered.push(port);
                    } else {
                        state.unreachable = true;
                    }
                }
                Probe::Error => {}
            }
        }
        if answer == Probe::Error {
            self.errors.fetch_add(1, Ordering::Relaxed);
        }
        if announce {
            self.found.fetch_add(1, Ordering::Relaxed);
            (self.notify)(Event::Found(self.plan.hosts[host]));
        }
    }

    /// Runs one pass: every `ports` entry against every host in `active`. Jobs go port-major — port 1 across the
    /// whole range, then port 2 — so one host is never hit by many probes in a row.
    fn pass(&self, number: u8, active: &[usize], ports: &[u16], workers: usize) {
        let total = active.len() as u64 * ports.len() as u64;
        if total == 0 {
            return;
        }
        let hosts = active.len() as u64;
        let next = AtomicU64::new(0);
        let done = AtomicU64::new(0);
        let work = || loop {
            if self.halted() {
                break;
            }
            let job = next.fetch_add(1, Ordering::Relaxed);
            if job >= total {
                break;
            }
            let host = active[(job % hosts) as usize];
            let port = ports[(job / hosts) as usize];
            done.fetch_add(1, Ordering::Relaxed);
            if self.states[host].lock().unwrap_or_else(|e| e.into_inner()).unreachable {
                continue;
            }
            let answer = (self.prober)(self.plan.hosts[host], port, self.plan.timeout);
            if self.probes.fetch_add(1, Ordering::Relaxed) + 1 > MAX_TOTAL_PROBES {
                self.capped.store(true, Ordering::Relaxed);
            }
            self.record(host, port, answer);
        };
        let progress = || {
            (self.notify)(Event::Progress {
                pass: number,
                done: done.load(Ordering::Relaxed).min(total),
                total,
                found: self.found.load(Ordering::Relaxed),
            });
        };
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..workers.min(total as usize).max(1))
                .filter_map(|_| std::thread::Builder::new().stack_size(256 * 1024).spawn_scoped(scope, work).ok())
                .collect();
            if handles.is_empty() {
                // No thread could be made: do the work here rather than report a scan that did not happen.
                work();
                return;
            }
            loop {
                let finished = handles.iter().all(|h| h.is_finished());
                progress();
                if finished {
                    break;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        });
    }
}

/// Runs the whole plan. `prober` is the real TCP probe in production and a script in the tests, so the two-pass
/// logic, the cancellation and the bookkeeping are exercised without a network.
pub(crate) fn run(plan: &Plan, cancel: &AtomicBool, workers: usize, prober: &(dyn Fn(Ipv4Addr, u16, Duration) -> Probe + Sync), notify: &(dyn Fn(Event) + Sync)) -> Report {
    let shared = Shared {
        plan,
        states: plan.hosts.iter().map(|_| std::sync::Mutex::new(HostState::default())).collect(),
        cancel,
        capped: AtomicBool::new(false),
        prober,
        notify,
        probes: AtomicU64::new(0),
        errors: AtomicU64::new(0),
        found: std::sync::atomic::AtomicUsize::new(0),
    };
    let everyone: Vec<usize> = (0..plan.hosts.len()).collect();
    shared.pass(1, &everyone, &plan.first_pass, workers);
    if !shared.halted() && !plan.second_pass.is_empty() {
        let answered: Vec<usize> =
            everyone.iter().copied().filter(|&i| shared.states[i].lock().unwrap_or_else(|e| e.into_inner()).answered).collect();
        shared.pass(2, &answered, &plan.second_pass, workers);
    }
    let mut found = Vec::new();
    for (i, state) in shared.states.iter().enumerate() {
        let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
        if !s.answered {
            continue;
        }
        let mut entry = Found { ip: Some(plan.hosts[i]), open: std::mem::take(&mut s.open), closed: std::mem::take(&mut s.closed), filtered: std::mem::take(&mut s.filtered) };
        for list in [&mut entry.open, &mut entry.closed, &mut entry.filtered] {
            list.sort_unstable();
            list.dedup();
        }
        found.push(entry);
    }
    Report {
        found,
        probes: shared.probes.load(Ordering::Relaxed),
        errors: shared.errors.load(Ordering::Relaxed),
        stopped: cancel.load(Ordering::Relaxed),
        capped: shared.capped.load(Ordering::Relaxed),
    }
}

// ── what the page receives ─────────────────────────────────────────────────────────────────────

/// When the person asked for ports, the report speaks only of those: the common ports pass 1 added to find the
/// host are a means, not an answer. With no ports the common set IS the answer.
fn visible(list: &[u16], plan: &Plan) -> Vec<u16> {
    if plan.ports.is_empty() {
        list.to_vec()
    } else {
        list.iter().copied().filter(|p| plan.ports.contains(p)).collect()
    }
}

pub(crate) fn done_event(id: u64, plan: &Plan, report: &Report, duration_ms: u64) -> serde_json::Value {
    let hosts: Vec<serde_json::Value> = report
        .found
        .iter()
        .filter_map(|f| {
            Some(serde_json::json!({
                "ip": f.ip?.to_string(),
                "open": visible(&f.open, plan),
                "closed": compact_ports(&visible(&f.closed, plan)),
                "filtered": compact_ports(&visible(&f.filtered, plan)),
            }))
        })
        .collect();
    serde_json::json!({
        "type": "ipv4_search_done",
        "scanId": id,
        "stopped": report.stopped,
        "capped": report.capped,
        "cidr": plan.cidr.label(),
        "addresses": plan.hosts.len(),
        "skipped": plan.skipped,
        "ports": compact_ports(&plan.ports),
        "discoveryPorts": compact_ports(&plan.first_pass),
        "portCount": plan.ports.len(),
        "found": hosts.len(),
        "probes": report.probes,
        "errors": report.errors,
        "durationMs": duration_ms,
        "hosts": hosts,
    })
}

pub(crate) fn confirmation_text(plan: &Plan) -> String {
    let ports = if plan.ports.is_empty() {
        format!("nenhuma informada — só descobrir quem responde (portas comuns: {})", compact_ports(&plan.first_pass))
    } else {
        format!("{} ({} portas)", compact_ports(&plan.ports), plan.ports.len())
    };
    let worst = plan.first_pass_probes() + plan.hosts.len() as u64 * plan.second_pass.len() as u64;
    format!(
        "Faixa: {}\nEndereços: {}\nPortas: {}\nTempo limite por conexão: {} ms\nAté {} tentativas de conexão TCP.\n\n\
         O ShvIA vai tentar abrir conexões a partir deste computador e fechá-las em seguida, sem enviar dados. \
         Faça isso apenas em redes que você administra ou que tem autorização para testar. Continuar?",
        plan.cidr.label(),
        plan.hosts.len(),
        ports,
        plan.timeout.as_millis(),
        worst,
    )
}

// ── the one scan at a time ─────────────────────────────────────────────────────────────────────

struct Active {
    id: u64,
    window: String,
    cancel: Arc<AtomicBool>,
}

static ACTIVE: OnceLock<Mutex<Option<Active>>> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn slot() -> &'static Mutex<Option<Active>> {
    ACTIVE.get_or_init(|| Mutex::new(None))
}

/// Takes the one slot BEFORE the dialog, so a second request cannot open a second dialog behind the first.
fn reserve(window: &str) -> Result<(u64, Arc<AtomicBool>), Refusal> {
    let mut guard = slot().lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_some() {
        return Err(Refusal::Busy);
    }
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let cancel = Arc::new(AtomicBool::new(false));
    *guard = Some(Active { id, window: window.to_string(), cancel: cancel.clone() });
    Ok((id, cancel))
}

fn release(id: u64) {
    let mut guard = slot().lock().unwrap_or_else(|e| e.into_inner());
    if guard.as_ref().is_some_and(|a| a.id == id) {
        *guard = None;
    }
}

/// Stops the scan in progress, if any. Returns whether there was one. The `ipv4_search_done` event, with
/// `stopped: true`, follows from the scan's own thread.
pub(crate) fn stop() -> bool {
    match slot().lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        Some(a) => {
            a.cancel.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

/// Called when the window starts a new document or the app quits: the page that asked is gone, so nobody is left to
/// read the result and nobody to press Stop.
pub fn cancelar_varredura(window: Option<&str>) {
    if let Some(a) = slot().lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        if window.is_none_or(|w| w == a.window) {
            a.cancel.store(true, Ordering::Relaxed);
        }
    }
}

/// How many probes may be in flight. Each is one descriptor, and a macOS GUI app starts with a soft limit of 256, shared
/// with the webview and everything else the app has open — so raise it when the hard limit allows, and fall back to a
/// number that fits the default when it does not.
// `rlim_t` is not `u64` everywhere the app builds, so the casts below are needed on some targets and redundant on this one.
#[allow(clippy::unnecessary_cast)]
fn worker_count() -> usize {
    #[cfg(unix)]
    {
        let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        // SAFETY: `limit` is a valid, initialised `rlimit` that lives across both calls.
        let soft = unsafe {
            if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) != 0 {
                return 32;
            }
            let wanted = (limit.rlim_max as u64).min(4096);
            if (limit.rlim_cur as u64) < wanted {
                limit.rlim_cur = wanted as libc::rlim_t;
                if libc::setrlimit(libc::RLIMIT_NOFILE, &limit) != 0 {
                    let _ = libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit);
                }
            }
            limit.rlim_cur as u64
        };
        match soft {
            s if s >= 1024 => 128,
            s if s >= 512 => 64,
            _ => 32,
        }
    }
    #[cfg(not(unix))]
    {
        128
    }
}

fn emit(window: &WebviewWindow, event: &serde_json::Value) {
    let js = format!("window.__shviaCode&&window.__shviaCode._emit(JSON.parse({}))", js_str(&event.to_string()));
    let w = window.clone();
    let app = window.app_handle().clone();
    let _ = app.run_on_main_thread(move || {
        let _ = w.eval(&js);
    });
}

fn confirm(window: &WebviewWindow, plan: &Plan) -> bool {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
    window
        .app_handle()
        .dialog()
        .message(confirmation_text(plan))
        .title("Autorizar varredura de rede")
        .buttons(MessageDialogButtons::OkCancel)
        .blocking_show()
}

/// `ipv4SearchStart({target, ports?, timeoutMs?, probeAll?})`. Validates, reserves the slot, asks the person in a native
/// dialog and only then starts. Returns at once with the scan's number; progress, hosts as they answer and the final
/// report arrive as events (`ipv4_search_progress`, `ipv4_search_host`, `ipv4_search_done`). The error is an
/// `E_SCAN_*: text` string.
pub(super) fn start(window: &WebviewWindow, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let target = v.get("target").and_then(|x| x.as_str()).unwrap_or("");
    let ports = v.get("ports").and_then(|x| x.as_str()).unwrap_or("");
    let timeout = match v.get("timeoutMs") {
        None | Some(serde_json::Value::Null) => None,
        Some(x) => Some(x.as_u64().ok_or_else(|| Refusal::InvalidOptions.message())?),
    };
    let probe_all = v.get("probeAll").and_then(|x| x.as_bool()).unwrap_or(false);
    let plan = plan(target, ports, timeout, probe_all).map_err(Refusal::message)?;

    let label = window.label().to_string();
    let (id, cancel) = reserve(&label).map_err(Refusal::message)?;
    if !confirm(window, &plan) {
        release(id);
        return Err(Refusal::Declined.message());
    }
    if window.app_handle().get_webview_window(&label).is_none() {
        release(id);
        return Err(Refusal::WindowChanged.message());
    }

    let summary = serde_json::json!({
        "scanId": id,
        "cidr": plan.cidr.label(),
        "addresses": plan.hosts.len(),
        "ports": compact_ports(&plan.ports),
        "discoveryPorts": compact_ports(&plan.first_pass),
    });
    let w = window.clone();
    std::thread::spawn(move || {
        let started = Instant::now();
        let watcher = w.clone();
        let label = label;
        let notify = move |event: Event| {
            // The window is gone: nobody will read this, so stop instead of finishing a scan for no one.
            if watcher.app_handle().get_webview_window(&label).is_none() {
                cancel_by_id(id);
                return;
            }
            let json = match event {
                Event::Found(ip) => serde_json::json!({"type": "ipv4_search_host", "scanId": id, "ip": ip.to_string()}),
                Event::Progress { pass, done, total, found } => {
                    serde_json::json!({"type": "ipv4_search_progress", "scanId": id, "pass": pass, "done": done, "total": total, "found": found})
                }
            };
            emit(&watcher, &json);
        };
        let report = run(&plan, &cancel, worker_count(), &probe_tcp, &notify);
        emit(&w, &done_event(id, &plan, &report, started.elapsed().as_millis() as u64));
        release(id);
    });
    Ok(summary)
}

fn cancel_by_id(id: u64) {
    if let Some(a) = slot().lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        if a.id == id {
            a.cancel.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashSet};
    use std::net::TcpListener;
    use std::sync::Mutex as StdMutex;

    fn ip(s: &str) -> Ipv4Addr {
        s.parse().unwrap()
    }

    // ── target ──

    #[test]
    fn the_example_range_is_a_private_slash_22_with_1022_hosts() {
        let p = plan("10.100.100.0/22", "", None, false).unwrap();
        assert_eq!(p.cidr.label(), "10.100.100.0/22");
        assert_eq!(p.hosts.len(), 1022);
        assert_eq!(p.hosts.first(), Some(&ip("10.100.100.1")));
        assert_eq!(p.hosts.last(), Some(&ip("10.100.103.254")));
    }

    #[test]
    fn host_bits_are_cleared_and_a_bare_address_is_a_slash_32() {
        assert_eq!(Cidr::parse("10.100.100.7/22").unwrap().label(), "10.100.100.0/22");
        let single = plan("10.100.100.5", "8006", None, false).unwrap();
        assert_eq!(single.hosts, vec![ip("10.100.100.5")]);
        assert_eq!(plan("10.100.100.4/31", "", None, false).unwrap().hosts.len(), 2);
    }

    #[test]
    fn malformed_targets_are_refused() {
        for bad in ["", " ", "10.100.100", "10.100.100.0/", "10.100.100.0/ 22", "10.100.100.0/+22", "10.100.100.0/33", "10.100.100.256/24",
            "010.100.100.1/24", "10.100.100.0/22/1", "::1", "fe80::1/64", "example.com", "10.100.100.0-10.100.100.9", "10.100.100.0/2a"]
        {
            assert_eq!(Cidr::parse(bad), Err(Refusal::InvalidTarget), "{bad:?}");
        }
    }

    #[test]
    fn a_range_wider_than_a_slash_16_is_too_large() {
        for wide in ["10.0.0.0/8", "10.0.0.0/15", "10.0.0.0/0"] {
            assert_eq!(plan(wide, "", None, false).unwrap_err(), Refusal::TooLarge, "{wide}");
        }
        assert!(plan("10.1.0.0/16", "", None, false).is_ok());
    }

    #[test]
    fn public_and_forbidden_ranges_are_refused_not_confirmed() {
        for public in ["8.8.8.0/24", "1.1.1.1", "172.32.0.0/16", "192.169.0.0/16", "100.128.0.0/16", "11.0.0.0/16"] {
            assert_eq!(plan(public, "80", None, false).unwrap_err(), Refusal::PublicRange, "{public}");
        }
        for never in ["127.0.0.0/16", "127.0.0.1", "169.254.0.0/16", "169.254.169.254", "224.0.0.0/16", "0.0.0.0/16", "255.255.255.255", "240.0.0.0/16"] {
            assert_eq!(plan(never, "80", None, false).unwrap_err(), Refusal::ForbiddenRange, "{never}");
        }
    }

    #[test]
    fn every_private_range_is_accepted() {
        for private in ["10.1.2.0/24", "172.16.0.0/16", "172.31.255.0/24", "192.168.0.0/24", "100.64.0.0/16", "100.127.255.0/24", "198.18.0.0/16", "198.19.0.0/24"] {
            assert!(plan(private, "80", None, false).is_ok(), "{private}");
        }
    }

    /// The whole-range check looks at the first and last address only. That is sound because every range this tool accepts
    /// is a /16 or narrower and every zone boundary is a /16 or wider — proved here for all 65,536 possible /16 blocks.
    #[test]
    fn a_slash_16_is_never_split_between_zones() {
        for block in 0..=u16::MAX {
            let base = u32::from(block) << 16;
            let first = Ipv4Addr::from(base);
            let last = Ipv4Addr::from(base | 0xffff);
            let middle = Ipv4Addr::from(base | 0x8000);
            assert_eq!(zone(first), zone(last), "{first}");
            assert_eq!(zone(first), zone(middle), "{first}");
        }
    }

    #[test]
    fn the_cloud_metadata_address_inside_the_cgnat_range_is_left_out() {
        let p = plan("100.100.100.0/24", "", None, false).unwrap();
        assert_eq!(p.skipped, 1);
        assert!(!p.hosts.contains(&ip("100.100.100.200")));
        assert_eq!(p.hosts.len(), 253);
    }

    // ── ports ──

    #[test]
    fn the_documented_example_expands_to_82_ports() {
        let ports = parse_ports("2000,8000-8080").unwrap();
        assert_eq!(ports.len(), 82);
        assert_eq!(ports[0], 2000);
        assert_eq!(ports[1], 8000);
        assert_eq!(*ports.last().unwrap(), 8080);
    }

    #[test]
    fn ports_are_sorted_deduplicated_and_tolerate_spaces() {
        assert_eq!(parse_ports(" 8006 , 22,22, 80 - 82 ").unwrap(), vec![22, 80, 81, 82, 8006]);
        assert_eq!(parse_ports("").unwrap(), Vec::<u16>::new());
        assert_eq!(parse_ports("   ").unwrap(), Vec::<u16>::new());
        assert_eq!(parse_ports("65535").unwrap(), vec![65535]);
    }

    #[test]
    fn malformed_ports_are_refused() {
        for bad in ["0", "65536", "-1", "80-", "-80", "a", "80,,81", "80,", ",80", "90-80", "80-90-100", "8o", "+80", "80.5", "99999999999"] {
            assert_eq!(parse_ports(bad), Err(Refusal::InvalidPorts), "{bad:?}");
        }
    }

    #[test]
    fn too_many_ports_are_refused_without_building_them() {
        assert_eq!(parse_ports("1-65535"), Err(Refusal::TooManyPorts));
        assert_eq!(parse_ports("1-1025"), Err(Refusal::TooManyPorts));
        assert_eq!(parse_ports("1-1024").unwrap().len(), 1024);
        assert_eq!(parse_ports("1-1024,2000"), Err(Refusal::TooManyPorts));
    }

    #[test]
    fn ports_print_back_as_ranges() {
        assert_eq!(compact_ports(&[]), "");
        assert_eq!(compact_ports(&[80]), "80");
        assert_eq!(compact_ports(&[8002, 80, 8000, 8001, 8001]), "80,8000-8002");
        assert_eq!(compact_ports(&[1, 2]), "1-2");
        assert_eq!(compact_ports(&[65534, 65535]), "65534-65535");
    }

    /// SHVIA-WEB's `public/js/ipv4-search.js` validates the same text for a good message. This copy of its vectors
    /// (`docs/NETWORK/ipv4-search-vectors.json` over there) is what keeps the two parsers from drifting apart: each
    /// side is tested against its own copy, so changing one without the other is a visible edit in both repositories.
    #[test]
    fn the_vectors_shared_with_the_page_agree_with_this_parser() {
        let vectors: serde_json::Value = serde_json::from_str(include_str!("ipv4_search_vectors.json")).expect("vectors parse");
        let slug = |r: Refusal| r.code().trim_start_matches("E_SCAN_").to_lowercase();
        let targets = vectors["targets"].as_array().expect("targets");
        assert!(targets.len() > 40);
        for case in targets {
            let input = case["input"].as_str().unwrap();
            match plan(input, "", None, false) {
                Ok(p) => {
                    assert_eq!(case["ok"], true, "{input:?} was accepted");
                    assert_eq!(p.cidr.label(), case["cidr"].as_str().unwrap(), "{input:?}");
                    assert_eq!((p.hosts.len() + p.skipped) as u64, case["addresses"].as_u64().unwrap(), "{input:?}");
                }
                Err(r) => {
                    assert_eq!(case["ok"], false, "{input:?} was refused as {r:?}");
                    assert_eq!(slug(r), case["code"].as_str().unwrap(), "{input:?}");
                }
            }
        }
        let ports = vectors["ports"].as_array().expect("ports");
        assert!(ports.len() > 20);
        for case in ports {
            let input = case["input"].as_str().unwrap();
            match parse_ports(input) {
                Ok(list) => {
                    assert_eq!(case["ok"], true, "{input:?} was accepted");
                    assert_eq!(compact_ports(&list), case["compact"].as_str().unwrap(), "{input:?}");
                    assert_eq!(list.len() as u64, case["count"].as_u64().unwrap(), "{input:?}");
                }
                Err(r) => {
                    assert_eq!(case["ok"], false, "{input:?} was refused as {r:?}");
                    assert_eq!(slug(r), case["code"].as_str().unwrap(), "{input:?}");
                }
            }
        }
    }

    // ── plan ──

    #[test]
    fn with_ports_pass_one_leads_with_the_persons_own_and_pass_two_has_the_rest() {
        let p = plan("10.100.100.0/24", "2000,8000-8080", None, false).unwrap();
        assert_eq!(p.first_pass, vec![2000, 8000, 8001, 8002, 443, 80, 22, 445]);
        assert_eq!(p.first_pass.len(), DISCOVERY_MAX);
        assert_eq!(p.second_pass.len(), 82 - 4);
        assert!(p.second_pass.iter().all(|x| !p.first_pass.contains(x)));
    }

    #[test]
    fn without_ports_it_is_discovery_only() {
        let p = plan("10.100.100.0/24", "", None, false).unwrap();
        assert_eq!(p.first_pass, DISCOVERY_PORTS.to_vec());
        assert!(p.second_pass.is_empty());
        assert!(p.ports.is_empty());
    }

    #[test]
    fn probing_everything_needs_ports_and_skips_the_shortcut() {
        assert_eq!(plan("10.100.100.0/24", "", None, true).unwrap_err(), Refusal::NothingToProbe);
        let p = plan("10.100.100.0/24", "8000-8010", None, true).unwrap();
        assert_eq!(p.first_pass.len(), 11);
        assert!(p.second_pass.is_empty());
    }

    #[test]
    fn a_pass_one_that_is_too_big_is_refused_up_front() {
        // /16 × 16 ports > 1,000,000 attempts
        assert_eq!(plan("10.1.0.0/16", "1-16", None, true).unwrap_err(), Refusal::TooManyProbes);
        assert!(plan("10.1.0.0/16", "", None, false).is_ok()); // 65,534 × 8 = 524,272
    }

    #[test]
    fn the_time_limit_has_bounds() {
        assert_eq!(plan("10.1.1.0/24", "", Some(199), false).unwrap_err(), Refusal::InvalidOptions);
        assert_eq!(plan("10.1.1.0/24", "", Some(5001), false).unwrap_err(), Refusal::InvalidOptions);
        assert_eq!(plan("10.1.1.0/24", "", Some(200), false).unwrap().timeout, Duration::from_millis(200));
        assert_eq!(plan("10.1.1.0/24", "", None, false).unwrap().timeout, Duration::from_millis(800));
    }

    #[test]
    fn the_confirmation_names_what_will_be_scanned() {
        let text = confirmation_text(&plan("10.100.100.0/22", "2000,8000-8080", Some(1500), false).unwrap());
        for needle in ["10.100.100.0/22", "1022", "2000,8000-8080", "82 portas", "1500 ms", "sem enviar dados"] {
            assert!(text.contains(needle), "missing {needle:?} in {text}");
        }
        let discovery = confirmation_text(&plan("10.100.100.0/24", "", None, false).unwrap());
        assert!(discovery.contains("só descobrir quem responde"));
    }

    #[test]
    fn every_refusal_has_a_stable_code_and_text() {
        let all = [
            Refusal::InvalidTarget, Refusal::PublicRange, Refusal::ForbiddenRange, Refusal::TooLarge, Refusal::InvalidPorts, Refusal::TooManyPorts,
            Refusal::TooManyProbes, Refusal::InvalidOptions, Refusal::NothingToProbe, Refusal::Busy, Refusal::Declined, Refusal::WindowChanged,
        ];
        let codes: HashSet<&str> = all.iter().map(|r| r.code()).collect();
        assert_eq!(codes.len(), all.len());
        for r in all {
            assert!(r.code().starts_with("E_SCAN_"));
            assert!(r.message().starts_with(&format!("{}: ", r.code())));
        }
    }

    // ── engine, with a scripted network ──

    fn quiet() -> impl Fn(Event) + Sync {
        |_| {}
    }

    /// A network where `answers` says what each (host, port) does; everything else times out. Records every probe made.
    type CallLog = std::sync::Arc<StdMutex<Vec<(Ipv4Addr, u16)>>>;

    fn scripted(answers: Vec<(&'static str, u16, Probe)>) -> (impl Fn(Ipv4Addr, u16, Duration) -> Probe + Sync, CallLog) {
        let calls = std::sync::Arc::new(StdMutex::new(Vec::new()));
        let log = calls.clone();
        let prober = move |host: Ipv4Addr, port: u16, _t: Duration| {
            log.lock().unwrap().push((host, port));
            answers.iter().find(|(h, p, _)| ip(h) == host && *p == port).map(|&(_, _, a)| a).unwrap_or(Probe::Filtered)
        };
        (prober, calls)
    }

    #[test]
    fn hosts_that_answer_are_found_and_hosts_that_only_time_out_are_not() {
        let p = plan("10.100.100.0/29", "8006", Some(200), false).unwrap(); // hosts .1 - .6
        let (prober, _) = scripted(vec![
            ("10.100.100.2", 8006, Probe::Open),
            ("10.100.100.3", 8006, Probe::Closed), // alive, nothing listens
            ("10.100.100.4", 22, Probe::Open),     // alive on a common port, 8006 silent
        ]);
        let report = run(&p, &AtomicBool::new(false), 4, &prober, &quiet());
        let ips: Vec<String> = report.found.iter().map(|f| f.ip.unwrap().to_string()).collect();
        assert_eq!(ips, vec!["10.100.100.2", "10.100.100.3", "10.100.100.4"]);
        assert_eq!(report.found[0].open, vec![8006]);
        assert_eq!(report.found[1].closed, vec![8006]);
        assert!(report.found[2].open.contains(&22));
        assert!(!report.stopped && !report.capped);
    }

    #[test]
    fn pass_two_probes_only_the_hosts_that_answered_pass_one() {
        let p = plan("10.100.100.0/29", "8000-8020", Some(200), false).unwrap();
        // 8000 is in pass 1 (first four of the person's own ports); 8010 is pass 2.
        let (prober, calls) = scripted(vec![("10.100.100.2", 8000, Probe::Closed), ("10.100.100.2", 8010, Probe::Open)]);
        let report = run(&p, &AtomicBool::new(false), 4, &prober, &quiet());
        assert_eq!(report.found.len(), 1);
        assert_eq!(report.found[0].open, vec![8010]);
        let calls = calls.lock().unwrap();
        let on_silent_host = calls.iter().filter(|(h, port)| *h == ip("10.100.100.5") && *port == 8010).count();
        assert_eq!(on_silent_host, 0, "pass 2 must not touch a host that never answered");
        assert!(calls.iter().any(|(h, port)| *h == ip("10.100.100.2") && *port == 8010));
        // a probe is never repeated
        let unique: BTreeSet<_> = calls.iter().collect();
        assert_eq!(unique.len(), calls.len());
    }

    #[test]
    fn probe_all_reaches_a_host_that_drops_the_common_ports() {
        // .5 only answers 8050; the shortcut would never ask it.
        let answers = vec![("10.100.100.5", 8050, Probe::Open)];
        let shortcut = plan("10.100.100.0/29", "8040-8060", Some(200), false).unwrap();
        let (prober, _) = scripted(answers.clone());
        assert!(run(&shortcut, &AtomicBool::new(false), 4, &prober, &quiet()).found.is_empty());
        let everything = plan("10.100.100.0/29", "8040-8060", Some(200), true).unwrap();
        let (prober, _) = scripted(answers);
        let report = run(&everything, &AtomicBool::new(false), 4, &prober, &quiet());
        assert_eq!(report.found.len(), 1);
        assert_eq!(report.found[0].open, vec![8050]);
    }

    #[test]
    fn an_unreachable_address_stops_being_probed() {
        let p = plan("10.100.100.0/30", "", Some(200), false).unwrap(); // .1 and .2
        let (prober, calls) = scripted(
            DISCOVERY_PORTS.iter().map(|&port| ("10.100.100.1", port, Probe::Unreachable)).collect(),
        );
        // with one worker the order is deterministic: port-major, so .1 is hit first on every port
        let report = run(&p, &AtomicBool::new(false), 1, &prober, &quiet());
        assert!(report.found.is_empty());
        let on_dead = calls.lock().unwrap().iter().filter(|(h, _)| *h == ip("10.100.100.1")).count();
        assert_eq!(on_dead, 1, "after the first 'no route', the other ports are skipped");
    }

    #[test]
    fn a_probe_that_could_not_be_made_is_an_error_not_a_filtered_port() {
        let p = plan("10.100.100.0/30", "8006", Some(200), false).unwrap();
        let (prober, _) = scripted(vec![("10.100.100.1", 8006, Probe::Error), ("10.100.100.2", 8006, Probe::Open)]);
        let report = run(&p, &AtomicBool::new(false), 2, &prober, &quiet());
        assert_eq!(report.errors, 1);
        assert_eq!(report.found.len(), 1);
        assert!(report.found.iter().all(|f| f.filtered.is_empty() || f.ip != Some(ip("10.100.100.1"))));
    }

    #[test]
    fn cancelling_stops_the_scan_and_says_so() {
        let p = plan("10.100.0.0/24", "", Some(200), false).unwrap();
        let cancel = AtomicBool::new(false);
        let seen = std::sync::atomic::AtomicUsize::new(0);
        let prober = |_: Ipv4Addr, _: u16, _: Duration| {
            if seen.fetch_add(1, Ordering::Relaxed) == 20 {
                cancel.store(true, Ordering::Relaxed);
            }
            Probe::Filtered
        };
        let report = run(&p, &cancel, 4, &prober, &quiet());
        assert!(report.stopped);
        assert!(report.probes < 254 * 8 / 2, "stopped early: {}", report.probes);
    }

    #[test]
    fn found_events_announce_each_host_once_and_progress_reaches_the_end() {
        let p = plan("10.100.100.0/29", "8006", Some(200), false).unwrap();
        let (prober, _) = scripted(vec![("10.100.100.2", 8006, Probe::Open), ("10.100.100.2", 22, Probe::Open), ("10.100.100.3", 8006, Probe::Closed)]);
        let events = StdMutex::new(Vec::<String>::new());
        let notify = |e: Event| {
            events.lock().unwrap().push(match e {
                Event::Found(ip) => format!("found {ip}"),
                Event::Progress { pass, done, total, .. } => format!("progress {pass} {done}/{total}"),
            })
        };
        run(&p, &AtomicBool::new(false), 3, &prober, &notify);
        let events = events.lock().unwrap();
        assert_eq!(events.iter().filter(|e| e.as_str() == "found 10.100.100.2").count(), 1);
        assert_eq!(events.iter().filter(|e| e.as_str() == "found 10.100.100.3").count(), 1);
        let last_pass_one = events.iter().rev().find(|e| e.starts_with("progress 1")).unwrap();
        assert_eq!(last_pass_one, "progress 1 48/48"); // 6 hosts × 8 pass-1 ports
    }

    #[test]
    fn the_report_speaks_only_of_the_ports_that_were_asked_for() {
        let p = plan("10.100.100.0/29", "8006", Some(200), false).unwrap();
        let (prober, _) = scripted(vec![("10.100.100.2", 8006, Probe::Closed), ("10.100.100.2", 443, Probe::Open), ("10.100.100.2", 22, Probe::Closed)]);
        let report = run(&p, &AtomicBool::new(false), 2, &prober, &quiet());
        let event = done_event(7, &p, &report, 1234);
        assert_eq!(event["type"], "ipv4_search_done");
        assert_eq!(event["scanId"], 7);
        assert_eq!(event["found"], 1);
        let host = &event["hosts"][0];
        assert_eq!(host["ip"], "10.100.100.2");
        assert_eq!(host["open"], serde_json::json!([]), "443 was only a way to find the host");
        assert_eq!(host["closed"], "8006");
        assert_eq!(host["filtered"], "");
        assert_eq!(event["ports"], "8006");
        assert_eq!(event["cidr"], "10.100.100.0/29");
    }

    #[test]
    fn a_discovery_report_lists_the_common_ports_that_answered() {
        let p = plan("10.100.100.0/29", "", Some(200), false).unwrap();
        let (prober, _) = scripted(vec![("10.100.100.2", 443, Probe::Open), ("10.100.100.2", 22, Probe::Open), ("10.100.100.2", 80, Probe::Closed)]);
        let report = run(&p, &AtomicBool::new(false), 2, &prober, &quiet());
        let host = &done_event(1, &p, &report, 0)["hosts"][0];
        assert_eq!(host["open"], serde_json::json!([22, 443]));
        assert_eq!(host["closed"], "80");
    }

    // ── the real probe, on this machine ──

    #[test]
    fn the_real_probe_tells_open_from_closed_without_sending_anything() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let open = listener.local_addr().unwrap().port();
        let closed = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        }; // dropped: nothing listens there any more
        let loopback = Ipv4Addr::LOCALHOST;
        assert_eq!(probe_tcp(loopback, open, Duration::from_millis(500)), Probe::Open);
        assert_eq!(probe_tcp(loopback, closed, Duration::from_millis(500)), Probe::Closed);
    }

    #[test]
    fn the_real_engine_reports_a_listening_port_on_this_machine() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let open = listener.local_addr().unwrap().port();
        // The policy refuses loopback as a TARGET; the engine itself is address-agnostic, which is what lets it be tested.
        let p = Plan {
            cidr: Cidr::parse("10.0.0.1").unwrap(),
            hosts: vec![Ipv4Addr::LOCALHOST],
            skipped: 0,
            ports: vec![open],
            first_pass: vec![open],
            second_pass: vec![],
            timeout: Duration::from_millis(500),
        };
        let report = run(&p, &AtomicBool::new(false), 2, &probe_tcp, &quiet());
        assert_eq!(report.found.len(), 1);
        assert_eq!(report.found[0].open, vec![open]);
    }

    #[test]
    fn only_one_scan_may_hold_the_slot() {
        let first = reserve("janela-teste").expect("slot is free");
        assert_eq!(reserve("outra").unwrap_err(), Refusal::Busy);
        cancelar_varredura(Some("outra"));
        assert!(!first.1.load(Ordering::Relaxed), "a different window cannot cancel it");
        cancelar_varredura(Some("janela-teste"));
        assert!(first.1.load(Ordering::Relaxed));
        release(first.0);
        let second = reserve("outra").expect("released");
        release(second.0);
        assert!(!stop());
    }
}
