//! Unprivileged-by-default packet capture sidecar for the Network/Traffic tool.
//!
//! This is the ONLY part of ShvIA Desktop that links libpcap/Npcap and the only part that ever
//! needs elevated OS capability (`cap_net_raw`/`cap_net_admin` on Linux, BPF device access on
//! macOS, Npcap on Windows). The main app (`src-tauri`) never depends on `pcap`; it only spawns this
//! binary (see `code_bridge::traffic`) and reads one JSON object per line from its stdout. Modeled
//! on Wireshark's own `dumpcap` split — see docs/TRAFFIC/README.md.
//!
//! Three subcommands, chosen so every privilege-sensitive step is explicit and observable from the
//! caller's side instead of being guessed from an exit code:
//!   --list-interfaces                                      (no capability needed)
//!   --check-permission [--interface <id>]                  (probes, never captures)
//!   --capture --interface <id> [--filter <bpf>] --out <path.pcapng> [--max-bytes <n>]
//!
//! `--capture` lives exactly as long as its stdin stays open. The parent asks it to stop by closing
//! the pipe — which also finalizes the file and reports the kernel's drop counter — and if the
//! parent dies, the pipe closes with it, so a crashed app never leaves a privileged capture running.
//!
//! Lines on stdout (all JSON objects):
//!   {"interfaces":[…]}                                     --list-interfaces
//!   {"status":"ready"|"needs_permission"|"error", …}       --check-permission
//!   {"status":"started","interface":…,"linktype":…}        capture is open and the file exists
//!   {"packets":[{id,time,src,dst,protocol,length,summary,hex}…],"total":N,"notShown":M,"flushed":B}
//!   {"status":"stopped","reason":"requested"|"size_limit"|"error","total":N,"bytes":B,"kernelDropped":D}
//!   {"error":"text","code":"E_…"}                          the capture could not start / had to end

mod args;
mod parse;

use std::borrow::Cow;
use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Read, Write};
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use args::Args;
use parse::{hex_dump, relative_time, summarize, LinkKind, PacketView, HEX_BYTES_SENT};
use pcap_file::pcapng::blocks::enhanced_packet::EnhancedPacketBlock;
use pcap_file::pcapng::blocks::interface_description::{InterfaceDescriptionBlock, InterfaceDescriptionOption};
use pcap_file::pcapng::PcapNgWriter;
use pcap_file::DataLink;
use serde_json::{json, Value};

/// How often a batch of rows is flushed to stdout (and the capture file to disk), however many
/// packets arrived: keeps the bridge's webview `eval` calls at ten a second under any load, and
/// makes Export-while-capturing see everything up to the last tick.
const BATCH_INTERVAL: Duration = Duration::from_millis(100);
/// Rows the page receives per tick. A flood (a port scan, a LAN transfer) is 50,000 packets a
/// second; the table cannot read that, so the rest are COUNTED (`notShown`) and kept in the file.
const UI_ROWS_PER_TICK: usize = 100;
const SNAPLEN: u32 = 65_535;
const PCAP_BUFFER_BYTES: i32 = 8 * 1024 * 1024;
const POLL_MS: i32 = 100;
/// How long the loop rests when the interface has nothing for it: the stop request and the tick
/// are noticed within this much, and an idle capture costs about a hundred wakeups a second.
const IDLE_SLEEP: Duration = Duration::from_millis(10);

const E_PERMISSION: &str = "E_CAPTURE_PERMISSION_DENIED";
const E_INTERFACE: &str = "E_INTERFACE_UNAVAILABLE";
const E_FILTER: &str = "E_CAPTURE_FILTER_INVALID";
const E_FAILED: &str = "E_CAPTURE_FAILED";
const E_WRITE: &str = "E_CAPTURE_WRITE";

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match Args::parse(&argv) {
        Ok(a) => a,
        Err(message) => {
            emit_error(E_FAILED, &message);
            return ExitCode::from(2);
        }
    };
    match parsed {
        Args::ListInterfaces => list_interfaces(),
        Args::CheckPermission { interface } => check_permission(interface.as_deref()),
        Args::Capture { interface, filter, out, max_bytes } => capture(&interface, filter.as_deref(), &out, max_bytes),
    }
}

/// Writes one JSON line. `false` means stdout is gone — the parent is dead — and the caller stops.
fn emit_line(value: &Value) -> bool {
    let mut out = io::stdout().lock();
    writeln!(out, "{value}").and_then(|()| out.flush()).is_ok()
}

fn emit_error(code: &str, message: &str) {
    emit_line(&json!({ "error": message, "code": code }));
}

fn list_interfaces() -> ExitCode {
    match pcap::Device::list() {
        Ok(devices) => {
            emit_line(&json!({ "interfaces": interface_items(devices) }));
            ExitCode::SUCCESS
        }
        Err(e) => {
            emit_error(E_FAILED, &format!("could not list interfaces: {e}"));
            ExitCode::FAILURE
        }
    }
}

/// Up, non-loopback devices first (what a person wants to look at), loopback next, the rest last.
/// `name` is what the page prints: the id on Linux/macOS (`eth0`), the adapter's own description on
/// Windows, where the id is `\Device\NPF_{GUID}`. `description` is the addresses it carries.
fn interface_items(devices: Vec<pcap::Device>) -> Vec<Value> {
    let mut items: Vec<(u8, Value)> = devices
        .into_iter()
        .map(|d| {
            let up = d.flags.is_up();
            let loopback = d.flags.is_loopback();
            let desc = d.desc.clone().filter(|s| !s.is_empty());
            let name = if cfg!(windows) { desc.clone().unwrap_or_else(|| d.name.clone()) } else { d.name.clone() };
            let mut parts: Vec<String> = Vec::new();
            if loopback {
                parts.push("loopback".into());
            }
            parts.extend(d.addresses.iter().map(|a| a.addr.to_string()));
            if !cfg!(windows) {
                parts.extend(desc);
            }
            let rank = match (up, loopback) {
                (true, false) => 0,
                (true, true) => 1,
                _ => 2,
            };
            (
                rank,
                json!({ "id": d.name, "name": name, "description": parts.join(", "), "isUp": up, "isLoopback": loopback }),
            )
        })
        .collect();
    items.sort_by_key(|(rank, _)| *rank);
    items.into_iter().map(|(_, v)| v).collect()
}

/// The id comes from a web page. It must name a device libpcap itself lists, and it can never be
/// something `--flag`-shaped.
fn valid_interface(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 256 || name.starts_with('-') || name.chars().any(char::is_control) {
        return Err(format!("{name:?} is not an interface name"));
    }
    match pcap::Device::list() {
        Ok(devices) if devices.iter().any(|d| d.name == name) => Ok(()),
        Ok(_) => Err(format!("this computer has no interface called {name:?}")),
        Err(e) => Err(format!("could not list interfaces: {e}")),
    }
}

/// Opens one interface briefly, reads nothing, closes it. Tells "ready to capture" apart from
/// "needs permission" without ever starting a real capture — the page calls this before showing
/// Start as usable.
fn check_permission(interface: Option<&str>) -> ExitCode {
    let name = match interface {
        Some(name) => match valid_interface(name) {
            Ok(()) => name.to_string(),
            Err(message) => {
                emit_line(&json!({ "status": "error", "message": message }));
                return ExitCode::SUCCESS;
            }
        },
        // The capability is the binary's, not a device's: any real device answers for all of them.
        None => match pcap::Device::list().ok().and_then(|ds| ds.into_iter().next()) {
            Some(d) => d.name,
            None => {
                emit_line(&json!({ "status": "error", "message": "this computer has no capture interface" }));
                return ExitCode::SUCCESS;
            }
        },
    };
    match pcap::Capture::from_device(name.as_str()).and_then(|c| c.promisc(false).timeout(1).open()) {
        Ok(_active) => {
            emit_line(&json!({ "status": "ready" }));
        }
        Err(e) => {
            emit_line(&json!({ "status": classify_open_error(&e), "message": e.to_string() }));
        }
    }
    ExitCode::SUCCESS
}

fn classify_open_error(e: &pcap::Error) -> &'static str {
    let msg = e.to_string().to_ascii_lowercase();
    // Measured on this machine, without `cap_net_raw`: libpcap's actual Linux message is
    // "socket: Operation not permitted" -- it contains neither "permission" nor "denied" nor the
    // literal string "eperm", so those three alone let a real EPERM fall through to the generic
    // "error". macOS's BPF message ("... Permission denied") and a literal "EPERM" stay as a net
    // for wordings this build has not produced.
    if msg.contains("permission") || msg.contains("denied") || msg.contains("eperm") || msg.contains("not permitted") {
        "needs_permission"
    } else {
        "error"
    }
}

/// The capture file is private to the person who started it: a capture can hold anything that
/// crossed the wire, so no other local user may read it. `create_new` also refuses to follow or
/// overwrite a file that is already there.
fn create_private_file(path: &str) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// One buffered file shared by the pcapng writer and the tick that flushes it: the writer takes
/// ownership of what it writes to, and `PcapNgWriter` offers no way to flush it.
#[derive(Clone)]
struct SharedFile(Rc<RefCell<BufWriter<File>>>);

impl SharedFile {
    fn new(file: File) -> Self {
        Self(Rc::new(RefCell::new(BufWriter::with_capacity(256 * 1024, file))))
    }
}

impl Write for SharedFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.borrow_mut().flush()
    }
}

/// The rows waiting for the next tick, and the counters that go with them.
struct Stream {
    batch: Vec<PacketView>,
    started: Instant,
    total: u64,
    not_shown: u64,
    /// Bytes of the capture file that are on disk and end on a packet boundary: what Export may
    /// copy while the capture is still running.
    flushed: u64,
}

impl Stream {
    fn new() -> Self {
        Self { batch: Vec::with_capacity(UI_ROWS_PER_TICK), started: Instant::now(), total: 0, not_shown: 0, flushed: 0 }
    }

    fn due(&self) -> bool {
        self.started.elapsed() >= BATCH_INTERVAL
    }

    /// Sends the waiting rows (nothing at all when there are none). `false` = stdout is gone.
    fn flush(&mut self) -> bool {
        self.started = Instant::now();
        if self.batch.is_empty() {
            return true;
        }
        let line = json!({ "packets": self.batch, "total": self.total, "notShown": self.not_shown, "flushed": self.flushed });
        self.batch.clear();
        emit_line(&line)
    }
}

/// Why `capture` ended; the file is always complete up to the last whole packet.
enum Ending {
    Requested,
    SizeLimit,
    Failed,
}

impl Ending {
    fn reason(&self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::SizeLimit => "size_limit",
            Self::Failed => "error",
        }
    }
}

fn capture(interface: &str, filter: Option<&str>, out: &str, max_bytes: u64) -> ExitCode {
    if let Err(message) = valid_interface(interface) {
        emit_error(E_INTERFACE, &message);
        return ExitCode::FAILURE;
    }
    let device = match pcap::Capture::from_device(interface) {
        Ok(d) => d,
        Err(e) => {
            emit_error(E_INTERFACE, &format!("could not use interface {interface:?}: {e}"));
            return ExitCode::FAILURE;
        }
    };
    // Promiscuous mode stays off: this sees what reaches THIS computer's interface, which is also
    // what the page says. (The scope note in docs/TRAFFIC/README.md.)
    let mut active = match device
        .promisc(false)
        .snaplen(SNAPLEN as i32)
        .timeout(POLL_MS)
        .buffer_size(PCAP_BUFFER_BYTES)
        .open()
        .and_then(pcap::Capture::setnonblock)
    {
        Ok(a) => a,
        Err(e) => {
            let status = classify_open_error(&e);
            let code = if status == "needs_permission" { E_PERMISSION } else { E_INTERFACE };
            emit_line(&json!({ "status": status, "message": e.to_string(), "code": code, "error": e.to_string() }));
            return ExitCode::FAILURE;
        }
    };

    if let Some(bpf) = filter {
        if let Err(e) = active.filter(bpf, true) {
            emit_error(E_FILTER, &format!("invalid capture filter {bpf:?}: {e}"));
            return ExitCode::FAILURE;
        }
    }

    let file = match create_private_file(out) {
        Ok(f) => f,
        Err(e) => {
            emit_error(E_WRITE, &format!("could not create {out:?}: {e}"));
            return ExitCode::FAILURE;
        }
    };
    let dlt = active.get_datalink().0;
    let link = LinkKind::from_dlt(dlt);
    let shared = SharedFile::new(file);
    let mut bytes: u64 = 0;
    let mut writer = match start_pcapng(shared.clone(), interface, dlt) {
        Ok((w, n)) => {
            bytes += n;
            w
        }
        Err(message) => {
            emit_error(E_WRITE, &message);
            let _ = std::fs::remove_file(out);
            return ExitCode::FAILURE;
        }
    };
    if !emit_line(&json!({ "status": "started", "interface": interface, "linktype": dlt, "snaplen": SNAPLEN })) {
        let _ = std::fs::remove_file(out);
        return ExitCode::FAILURE;
    }

    let stop = watch_stdin();
    let mut stream = Stream::new();
    let mut first_us: Option<u64> = None;
    let mut ending = Ending::Requested;

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        match active.next_packet() {
            Ok(packet) => {
                let ts_us = header_time_us(packet.header);
                let first = *first_us.get_or_insert(ts_us);
                let block = EnhancedPacketBlock {
                    interface_id: 0,
                    timestamp: Duration::from_micros(ts_us),
                    original_len: packet.header.len,
                    data: Cow::Borrowed(packet.data),
                    options: Vec::new(),
                };
                match writer.write_pcapng_block(block) {
                    Ok(n) => bytes += n as u64,
                    Err(e) => {
                        // The file is what Export copies: a write that fails (disk full, quota)
                        // ends the capture instead of silently dropping frames the file then lacks.
                        emit_error(E_WRITE, &format!("could not write to the capture file: {e}"));
                        ending = Ending::Failed;
                        break;
                    }
                }
                stream.total += 1;
                if stream.batch.len() < UI_ROWS_PER_TICK {
                    stream.batch.push(row(stream.total, first, ts_us, link, packet.header.len, packet.data));
                } else {
                    stream.not_shown += 1;
                }
                if bytes >= max_bytes {
                    ending = Ending::SizeLimit;
                    break;
                }
            }
            // Non-blocking: "nothing arrived yet" is a normal answer, not an error. The loop then
            // sleeps a moment instead of letting libpcap block, because libpcap's own read timeout
            // is not dependable here — measured: on an idle interface `pcap_next_ex` did not return
            // for 8 s with a 100 ms timeout, so a stop request (stdin closed) would never be seen.
            Err(pcap::Error::TimeoutExpired) => std::thread::sleep(IDLE_SLEEP),
            Err(e) => {
                emit_error(E_FAILED, &format!("capture ended: {e}"));
                ending = Ending::Failed;
                break;
            }
        }
        if stream.due() {
            // The file first: a page that sees a row can already export it.
            if shared.clone().flush().is_err() {
                emit_error(E_WRITE, "could not write to the capture file");
                ending = Ending::Failed;
                break;
            }
            stream.flushed = bytes;
            if !stream.flush() {
                break; // the parent is gone
            }
        }
    }

    let _ = stream.flush();
    let flushed = shared.clone().flush();
    if flushed.is_err() && !matches!(ending, Ending::Failed) {
        emit_error(E_WRITE, "could not finish the capture file");
        ending = Ending::Failed;
    }
    let dropped = active.stats().map(|s| u64::from(s.dropped) + u64::from(s.if_dropped)).unwrap_or(0);
    emit_line(&json!({
        "status": "stopped",
        "reason": ending.reason(),
        "total": stream.total,
        "bytes": bytes,
        "kernelDropped": dropped,
    }));
    if matches!(ending, Ending::Failed) { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

/// The section header and the interface description. Without the second a pcapng file is invalid:
/// every packet block names an interface, and libpcap/Wireshark reject a file whose packets point
/// at one that was never described (and `pcap-file` itself refuses to write such a packet).
fn start_pcapng(
    sink: SharedFile,
    interface: &str,
    dlt: i32,
) -> Result<(PcapNgWriter<SharedFile>, u64), String> {
    let mut flusher = sink.clone();
    let mut writer = PcapNgWriter::new(sink).map_err(|e| format!("could not start the capture file: {e}"))?;
    let mut description = InterfaceDescriptionBlock::new(DataLink::from(dlt.max(0) as u32), SNAPLEN);
    description.options.push(InterfaceDescriptionOption::IfName(Cow::Owned(interface.to_string())));
    // `pcap-file` writes every packet's time as NANOseconds. A reader assumes microseconds when the
    // description says nothing (the pcapng default), so without this the exported file's dates were
    // a thousand times too large — caught by reading the file back with libpcap, a different reader.
    description.options.push(InterfaceDescriptionOption::IfTsResol(9));
    let written = writer
        .write_pcapng_block(description)
        .map_err(|e| format!("could not describe the interface in the capture file: {e}"))?;
    flusher.flush().map_err(|e| format!("could not write the capture file: {e}"))?;
    // The 28-byte section header `PcapNgWriter::new` wrote is part of the file too.
    Ok((writer, written as u64 + 28))
}

/// The packet's capture time in microseconds. `timeval`'s field widths are platform-defined
/// (`tv_usec` is `i64` on 64-bit Linux, where widening is a no-op clippy flags on this very build,
/// but `i32` on macOS/BSD, where it is a real conversion): keeping it working on every target
/// outranks silencing a lint that fires on only some of them, so the conversions live here.
#[allow(clippy::unnecessary_cast)]
fn header_time_us(header: &pcap::PacketHeader) -> u64 {
    timestamp_us(header.ts.tv_sec as i64, header.ts.tv_usec as i64)
}

fn timestamp_us(sec: i64, usec: i64) -> u64 {
    u64::try_from(sec.saturating_mul(1_000_000).saturating_add(usec)).unwrap_or(0)
}

fn row(id: u64, first_us: u64, ts_us: u64, link: LinkKind, wire_len: u32, data: &[u8]) -> PacketView {
    let s = summarize(link, data, wire_len);
    PacketView {
        id: id.to_string(),
        time: relative_time(first_us, ts_us),
        src: s.src,
        dst: s.dst,
        protocol: s.protocol,
        length: wire_len,
        summary: s.summary,
        hex: hex_dump(data, HEX_BYTES_SENT),
    }
}

/// The stop signal: the parent closing our stdin (or dying). Read and discard until EOF.
fn watch_stdin() -> Arc<AtomicBool> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    std::thread::spawn(move || {
        let mut sink = [0u8; 256];
        let mut stdin = io::stdin().lock();
        loop {
            match stdin.read(&mut sink) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
        flag.store(true, Ordering::Relaxed);
    });
    stop
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_capability_on_linux_is_classified_as_needs_permission() {
        // The literal message libpcap returns on this machine without `cap_net_raw` (confirmed by
        // actually running the binary against `lo`), which contains none of "permission", "denied"
        // or "eperm" -- the bug this test guards against.
        let e = pcap::Error::PcapError("socket: Operation not permitted".into());
        assert_eq!(classify_open_error(&e), "needs_permission");
    }

    #[test]
    fn an_unrelated_pcap_error_stays_a_generic_error() {
        let e = pcap::Error::PcapError("no such device exists".into());
        assert_eq!(classify_open_error(&e), "error");
    }

    #[test]
    fn an_interface_id_from_a_page_cannot_pose_as_a_flag_or_a_device_that_is_not_there() {
        assert!(valid_interface("").is_err());
        assert!(valid_interface("--out").is_err());
        assert!(valid_interface("eth0\n--out").is_err());
        assert!(valid_interface("no-such-interface-0").is_err());
    }

    #[test]
    fn timestamps_widen_without_overflow_or_underflow() {
        assert_eq!(timestamp_us(10, 250_000), 10_250_000);
        assert_eq!(timestamp_us(-5, 0), 0);
        assert_eq!(timestamp_us(i64::MAX, 999_999), (i64::MAX) as u64);
    }

    /// The file the page exports, written through the same code path as a live capture, read back
    /// by libpcap — an independent reader that insists on a section header, an interface
    /// description and well-formed packet blocks.
    #[test]
    fn the_capture_file_is_a_valid_pcapng_that_libpcap_reads_back_with_the_same_packets() {
        let dir = std::env::temp_dir().join(format!("traffic-capture-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roundtrip.pcapng");
        let file = create_private_file(path.to_str().unwrap()).unwrap();
        let shared = SharedFile::new(file);
        let (mut writer, _) = start_pcapng(shared.clone(), "test0", 1).unwrap();
        let frames: Vec<Vec<u8>> = (0u8..5).map(|i| vec![i; 60 + usize::from(i) * 10]).collect();
        for (i, frame) in frames.iter().enumerate() {
            let block = EnhancedPacketBlock {
                interface_id: 0,
                timestamp: Duration::from_micros(1_700_000_000_000_000 + i as u64 * 1_500),
                original_len: frame.len() as u32,
                data: Cow::Borrowed(frame),
                options: Vec::new(),
            };
            writer.write_pcapng_block(block).unwrap();
        }
        shared.clone().flush().unwrap();

        let mut reader = pcap::Capture::from_file(&path).expect("libpcap must accept the file");
        assert_eq!(reader.get_datalink().0, 1, "the interface description carries the link type");
        let mut seen = Vec::new();
        while let Ok(p) = reader.next_packet() {
            let us = header_time_us(p.header);
            seen.push((p.data.to_vec(), us / 1_000_000, us % 1_000_000));
        }
        assert_eq!(seen.len(), frames.len());
        for (i, (data, sec, usec)) in seen.iter().enumerate() {
            assert_eq!(data, &frames[i]);
            assert_eq!(*sec, 1_700_000_000);
            assert_eq!(*usec, i as u64 * 1_500, "microsecond timestamps survive the round trip");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "a capture is readable by its owner only");
        }
        assert!(create_private_file(path.to_str().unwrap()).is_err(), "an existing file is never overwritten");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
