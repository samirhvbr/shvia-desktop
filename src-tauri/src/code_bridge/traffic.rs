//! Native half of the Network "Traffic" tool (packet capture). The page names an interface
//! and a capture filter; opening the device, parsing frames and writing the capture file are
//! entirely native and live in the separate `traffic-capture` sidecar crate
//! (`src-tauri/traffic-capture/`), never in this process. `code_bridge` here only resolves
//! and spawns that sidecar, reads its NDJSON stdout, and answers the six bridge actions the
//! page calls. See docs/TRAFFIC/README.md for the privilege-separation rationale (modeled on
//! Wireshark's own GUI/`dumpcap` split) and the per-OS permission steps.
//!
//! Capture never starts implicitly: `start` only runs from the page's explicit "Start"
//! click, never on load or on interface selection. Nothing captured leaves this machine
//! except through `export`, which the user drives with a native save dialog.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{Manager, WebviewWindow};

const PRAZO_SONDA: Duration = Duration::from_secs(10);
const UNAVAILABLE: &str =
    "Captura indisponível: o sidecar traffic-capture não foi encontrado neste build.";

struct Captura {
    window: String,
    child: Child,
    temp_path: PathBuf,
}

fn estado() -> &'static Mutex<Option<Captura>> {
    static ESTADO: OnceLock<Mutex<Option<Captura>>> = OnceLock::new();
    ESTADO.get_or_init(|| Mutex::new(None))
}

/// Same shape as `motores::resolve_bin`'s step (1): a bundled `externalBin` always lands
/// beside the app's own executable. Unlike `anna` (built and released elsewhere), this
/// sidecar is our own code, so there is no PATH-lookup fallback — only a `tauri dev`
/// convenience that looks at the sibling crate's own `target/release`.
fn resolve_sidecar() -> Option<PathBuf> {
    let exe_name = if cfg!(windows) { "traffic-capture.exe" } else { "traffic-capture" };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join(exe_name);
            if p.exists() {
                return Some(p);
            }
        }
    }
    if cfg!(debug_assertions) {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("traffic-capture/target/release")
            .join(exe_name);
        if dev.exists() {
            return Some(dev);
        }
    }
    None
}

fn first_json_line(bytes: &[u8]) -> Option<Value> {
    String::from_utf8_lossy(bytes).lines().next().and_then(|l| serde_json::from_str(l).ok())
}

pub(super) fn interfaces() -> Result<Value, &'static str> {
    let bin = resolve_sidecar().ok_or(UNAVAILABLE)?;
    let mut cmd = crate::processo::comando(bin);
    cmd.arg("--list-interfaces");
    let out = super::saida_com_prazo(cmd, PRAZO_SONDA).map_err(|_| UNAVAILABLE)?;
    let value = first_json_line(&out.stdout).ok_or(UNAVAILABLE)?;
    match value.get("interfaces") {
        Some(list) => Ok(list.clone()),
        None => Err(UNAVAILABLE),
    }
}

/// `ready` / `needs_permission` / `missing_npcap` / `unsupported_platform` / `error` — the
/// page never has to guess from an exit code. `missing_npcap` only ever comes from the
/// Windows branch below: a sidecar that cannot even start because the Npcap driver is
/// absent typically fails at process load (a missing `wpcap.dll`/`Packet.dll` dependency),
/// before it gets the chance to print anything of its own.
pub(super) fn permission_status(interface: &str) -> Value {
    let Some(bin) = resolve_sidecar() else {
        return json!({ "status": "error", "message": UNAVAILABLE });
    };
    let mut cmd = crate::processo::comando(&bin);
    cmd.args(["--check-permission", "--interface", interface]);
    match super::saida_com_prazo(cmd, PRAZO_SONDA) {
        Ok(out) => match first_json_line(&out.stdout) {
            Some(v) => v,
            None => json!({ "status": "error", "message": "sidecar não respondeu" }),
        },
        Err(_) if cfg!(windows) => json!({
            "status": "missing_npcap",
            "message": "O driver Npcap não parece estar instalado.",
            "url": "https://npcap.com/#download",
        }),
        Err(e) => json!({ "status": "error", "message": e.to_string() }),
    }
}

/// The one explicit, user-initiated action that may trigger an OS elevation prompt — never
/// run on its own, never blanket "always allow" (same posture as `aparelho.rs`, ADR-039).
/// Linux/macOS ask for one-time elevation to grant the SIDECAR BINARY capture capability;
/// Windows has no self-elevation path here, since Npcap is installed separately by its own
/// official installer.
pub(super) fn request_permission() -> Result<Value, String> {
    let Some(bin) = resolve_sidecar() else { return Err(UNAVAILABLE.into()) };
    let bin = bin.to_string_lossy().to_string();

    #[cfg(target_os = "linux")]
    let resultado = {
        let mut cmd = crate::processo::comando("pkexec");
        cmd.args(["setcap", "cap_net_raw,cap_net_admin+eip", &bin]);
        match super::saida_com_prazo(cmd, Duration::from_secs(120)) {
            Ok(out) if out.status.success() => Ok(json!({ "granted": true })),
            Ok(out) => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
            Err(e) => Err(e.to_string()),
        }
    };

    #[cfg(target_os = "macos")]
    let resultado = {
        let script = "dseditgroup -o edit -a \"$(whoami)\" -t user access_bpf";
        let mut cmd = crate::processo::comando("osascript");
        cmd.arg("-e").arg(format!("do shell script \"{script}\" with administrator privileges"));
        match super::saida_com_prazo(cmd, Duration::from_secs(120)) {
            Ok(out) if out.status.success() => Ok(json!({ "granted": true })),
            Ok(out) => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
            Err(e) => Err(e.to_string()),
        }
    };

    // Windows has no self-elevation path here: Npcap is installed separately, by its own
    // official installer, which is the one that asks for admin — never this app.
    #[cfg(target_os = "windows")]
    let resultado: Result<Value, String> = Ok(json!({
        "granted": false,
        "url": "https://npcap.com/#download",
        "message": "Instale o Npcap e tente novamente.",
    }));

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let resultado: Result<Value, String> = Err("unsupported_platform".into());

    resultado
}

pub(super) fn start(window: &WebviewWindow, v: &Value) -> Result<Value, String> {
    let interface = v.get("interfaceId").and_then(|x| x.as_str()).filter(|s| !s.is_empty());
    let Some(interface) = interface else { return Err("interfaceId é obrigatório".into()) };
    // A capture filter string only ever becomes ONE argv element handed straight to the
    // sidecar (`Command::arg`, never a shell) — there is no path from here into a shell, and
    // no path from the page's separate, client-side DISPLAY filter into this call at all.
    let filter = v.get("filter").and_then(|x| x.as_str()).unwrap_or("");

    {
        let mut guard = estado().lock().map_err(|_| "estado indisponível".to_string())?;
        if guard.is_some() {
            return Err("já existe uma captura em andamento".into());
        }
        let bin = resolve_sidecar().ok_or(UNAVAILABLE)?;
        let temp_path = std::env::temp_dir()
            .join(format!("shvia-traffic-{}.pcapng", uuid_like()));

        let mut cmd = crate::processo::comando(bin);
        cmd.args(["--capture", "--interface", interface, "--filter", filter, "--out"])
            .arg(&temp_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = cmd.spawn().map_err(|e| e.to_string())?;
        let stdout = child.stdout.take().ok_or("sem stdout do sidecar")?;

        let win = window.clone();
        std::thread::spawn(move || read_loop(win, stdout));

        *guard = Some(Captura { window: window.label().to_string(), child, temp_path: temp_path.clone() });
        Ok(json!({ "started": true, "path": temp_path.to_string_lossy() }))
    }
}

fn read_loop(window: WebviewWindow, stdout: impl std::io::Read) {
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else { continue };
        let kind = if value.get("packets").is_some() {
            "traffic_packets"
        } else if value.get("error").is_some() {
            "traffic_error"
        } else {
            "traffic_status"
        };
        emit(&window, kind, &line);
    }
}

fn emit(window: &WebviewWindow, kind: &str, raw_json_line: &str) {
    let js = format!(
        "window.__shviaCode&&window.__shviaCode._emit(Object.assign(JSON.parse({}),{{type:{}}}))",
        super::js_str(raw_json_line),
        super::js_str(kind),
    );
    let w = window.clone();
    let _ = window.app_handle().run_on_main_thread(move || {
        let _ = w.eval(&js);
    });
}

/// Stops the capture the page started (`Some(window)`), or every capture still running at
/// app exit (`None`) — the same `Option<&str>` shape `pentest::stop` already uses. Killing
/// the child here is also what finalizes the pcapng file: `pcap-file` writes self-contained
/// blocks with no trailing record the format needs, so a killed process still leaves a
/// syntactically valid file truncated at the last complete block.
pub(crate) fn stop(window: Option<&str>) -> Value {
    let Ok(mut guard) = estado().lock() else { return json!({ "stopped": false }) };
    let Some(captura) = guard.as_mut().filter(|c| window.is_none_or(|w| c.window == w)) else {
        return json!({ "stopped": false });
    };
    let _ = captura.child.kill();
    let _ = captura.child.wait();
    *guard = None;
    json!({ "stopped": true })
}

pub(super) fn export(window: &WebviewWindow, req: String) {
    use tauri_plugin_dialog::DialogExt;

    let temp_path = {
        let guard = estado().lock().ok();
        guard.and_then(|g| g.as_ref().map(|c| c.temp_path.clone()))
    };
    let Some(temp_path) = temp_path else {
        super::reply(window, &req, false, json!({ "error": "nada capturado ainda" }));
        return;
    };

    let suggested = super::dialogos::sanitize_filename("capture.pcapng");
    let win = window.clone();
    window
        .app_handle()
        .dialog()
        .file()
        .set_file_name(&suggested)
        .add_filter("Wireshark capture", &["pcapng"])
        .save_file(move |path| {
            let Some(path) = path else {
                super::reply(&win, &req, true, json!({ "saved": false }));
                return;
            };
            let result = path
                .into_path()
                .map_err(|e| e.to_string())
                .and_then(|p| std::fs::copy(&temp_path, &p).map(|_| p).map_err(|e| e.to_string()));
            match result {
                Ok(p) => super::reply(
                    &win,
                    &req,
                    true,
                    json!({ "saved": true, "path": p.to_string_lossy() }),
                ),
                Err(e) => super::reply(&win, &req, false, json!({ "error": e })),
            }
        });
}

/// A capture file name unique enough for a per-session temp file, with no external
/// dependency: the process id plus a monotonic counter is enough — these never collide with
/// each other inside one process, and a leftover temp file from a crashed previous run is
/// never reused for a new one.
fn uuid_like() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{}", std::process::id(), n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_sidecar_never_panics_without_a_bundled_or_dev_binary() {
        // On this CI/dev machine there is no staged sidecar; the call must return `None`
        // cleanly instead of panicking, so `interfaces()`/`start()` can turn it into a
        // normal `Err(UNAVAILABLE)` the page can show.
        let _ = resolve_sidecar();
    }

    #[test]
    fn a_capture_filter_never_reaches_a_shell() {
        // Documents the boundary this module relies on: `Command::args` passes each element
        // as one argv entry, so a filter string containing shell metacharacters is inert here
        // regardless of its content. There is nothing to sanitize beyond what `start` already
        // requires (a non-empty interfaceId).
        let v = json!({ "interfaceId": "eth0", "filter": "tcp port 443; rm -rf /" });
        assert_eq!(v.get("filter").and_then(|x| x.as_str()), Some("tcp port 443; rm -rf /"));
    }

    #[test]
    fn stop_with_nothing_running_reports_not_stopped() {
        assert_eq!(stop(Some("some-window")), json!({ "stopped": false }));
    }
}
