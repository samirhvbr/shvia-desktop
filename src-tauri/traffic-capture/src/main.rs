//! Unprivileged-by-default packet capture sidecar for the Network/Traffic tool.
//!
//! This is the ONLY part of ShvIA Desktop that links libpcap/Npcap and the only part that
//! ever needs elevated OS capability (`cap_net_raw`/`cap_net_admin` on Linux, BPF group
//! access on macOS, Npcap on Windows). The main app (`src-tauri`) never depends on `pcap`;
//! it only spawns this binary (see `code_bridge::traffic`) and reads NDJSON from its stdout.
//! Modeled on Wireshark's own `dumpcap` split — see docs/TRAFFIC/README.md.
//!
//! Three subcommands, chosen so every privilege-sensitive step is explicit and observable
//! from the caller's side instead of being guessed from an exit code:
//!   --list-interfaces                                   (no capability needed)
//!   --check-permission --interface <id>                 (probes, never captures)
//!   --capture --interface <id> [--filter <bpf>] --out <path.pcapng>

mod args;
mod parse;

use std::borrow::Cow;
use std::io::{self, Write};
use std::process::ExitCode;
use std::time::Duration;

use args::Args;
use pcap_file::pcapng::blocks::enhanced_packet::EnhancedPacketBlock;
use pcap_file::pcapng::PcapNgWriter;

/// How often a batch of parsed summaries is flushed to stdout, regardless of how many
/// packets arrived — keeps the bridge's webview `eval` calls bounded under heavy traffic
/// without waiting for a fixed packet count that may never come on a quiet interface.
const BATCH_INTERVAL: Duration = Duration::from_millis(100);
const BATCH_MAX_PACKETS: usize = 200;

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match Args::parse(&argv) {
        Ok(a) => a,
        Err(message) => {
            emit_error(&message);
            return ExitCode::from(2);
        }
    };
    match parsed {
        Args::ListInterfaces => list_interfaces(),
        Args::CheckPermission { interface } => check_permission(&interface),
        Args::Capture { interface, filter, out } => capture(&interface, filter.as_deref(), &out),
    }
}

fn emit_line(value: &serde_json::Value) {
    let mut out = io::stdout();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

fn emit_error(message: &str) {
    emit_line(&serde_json::json!({ "error": message }));
}

fn list_interfaces() -> ExitCode {
    match pcap::Device::list() {
        Ok(devices) => {
            let items: Vec<_> = devices
                .into_iter()
                .map(|d| serde_json::json!({ "id": d.name, "description": d.desc.unwrap_or_default() }))
                .collect();
            emit_line(&serde_json::json!({ "interfaces": items }));
            ExitCode::SUCCESS
        }
        Err(e) => {
            emit_error(&format!("could not list interfaces: {e}"));
            ExitCode::FAILURE
        }
    }
}

/// Opens `interface` briefly, reads nothing, closes it. Tells "ready to capture" apart from
/// "needs permission" without ever starting a real capture — the page calls this before
/// showing Start as usable.
fn check_permission(interface: &str) -> ExitCode {
    match pcap::Capture::from_device(interface).and_then(|c| c.promisc(false).timeout(1).open()) {
        Ok(_active) => {
            emit_line(&serde_json::json!({ "status": "ready" }));
            ExitCode::SUCCESS
        }
        Err(e) => {
            emit_line(&serde_json::json!({ "status": classify_open_error(&e), "message": e.to_string() }));
            ExitCode::SUCCESS
        }
    }
}

fn classify_open_error(e: &pcap::Error) -> &'static str {
    let msg = e.to_string().to_ascii_lowercase();
    if msg.contains("permission") || msg.contains("denied") || msg.contains("eperm") {
        "needs_permission"
    } else {
        "error"
    }
}

fn capture(interface: &str, filter: Option<&str>, out: &str) -> ExitCode {
    let device = match pcap::Capture::from_device(interface) {
        Ok(d) => d,
        Err(e) => {
            emit_error(&format!("unknown interface {interface:?}: {e}"));
            return ExitCode::FAILURE;
        }
    };

    let mut active = match device.promisc(false).snaplen(65535).timeout(100).open() {
        Ok(a) => a,
        Err(e) => {
            emit_line(&serde_json::json!({
                "status": classify_open_error(&e),
                "message": e.to_string(),
            }));
            return ExitCode::FAILURE;
        }
    };

    if let Some(bpf) = filter {
        if let Err(e) = active.filter(bpf, true) {
            emit_error(&format!("invalid capture filter {bpf:?}: {e}"));
            return ExitCode::FAILURE;
        }
    }

    let file = match std::fs::File::create(out) {
        Ok(f) => f,
        Err(e) => {
            emit_error(&format!("could not create {out:?}: {e}"));
            return ExitCode::FAILURE;
        }
    };
    let mut writer = match PcapNgWriter::new(file) {
        Ok(w) => w,
        Err(e) => {
            emit_error(&format!("could not start {out:?}: {e}"));
            return ExitCode::FAILURE;
        }
    };

    let mut seq: u64 = 0;
    let mut batch = Vec::with_capacity(BATCH_MAX_PACKETS);
    let mut batch_started = std::time::Instant::now();

    loop {
        match active.next_packet() {
            Ok(packet) => {
                seq += 1;
                let ts_unix_ms = packet
                    .header
                    .ts
                    .tv_sec
                    .saturating_mul(1000)
                    .saturating_add(i64::from(packet.header.ts.tv_usec) / 1000) as u64;

                let block = EnhancedPacketBlock {
                    interface_id: 0,
                    timestamp: Duration::from_micros(
                        (packet.header.ts.tv_sec as u64).saturating_mul(1_000_000)
                            + packet.header.ts.tv_usec as u64,
                    ),
                    original_len: packet.header.len,
                    data: Cow::Borrowed(packet.data),
                    ..Default::default()
                };
                if writer.write_pcapng_block(block).is_err() {
                    // The file on disk is what Export copies; a write failure (disk full,
                    // permission revoked mid-capture) ends the capture rather than silently
                    // dropping frames the exported file would then be missing.
                    emit_error("could not write to the capture file; stopping");
                    return ExitCode::FAILURE;
                }

                batch.push(parse::summarize(seq, ts_unix_ms, packet.header.len, packet.data));
                if batch.len() >= BATCH_MAX_PACKETS || batch_started.elapsed() >= BATCH_INTERVAL {
                    flush_batch(&mut batch);
                    batch_started = std::time::Instant::now();
                }
            }
            // `timeout(100)` makes this a normal "nothing arrived yet" tick, not an error —
            // it is what lets the batch timer above fire even on a quiet interface.
            Err(pcap::Error::TimeoutExpired) => {
                if !batch.is_empty() && batch_started.elapsed() >= BATCH_INTERVAL {
                    flush_batch(&mut batch);
                    batch_started = std::time::Instant::now();
                }
            }
            Err(e) => {
                flush_batch(&mut batch);
                emit_error(&format!("capture ended: {e}"));
                return ExitCode::FAILURE;
            }
        }
    }
}

fn flush_batch(batch: &mut Vec<parse::PacketSummary>) {
    if batch.is_empty() {
        return;
    }
    emit_line(&serde_json::json!({ "packets": batch }));
    batch.clear();
}
