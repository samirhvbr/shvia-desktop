//! Native half of the Network "Traffic" tool (packet capture). The page names an interface and a
//! capture filter; opening the device, parsing frames and writing the capture file are entirely
//! native and live in the separate `traffic-capture` sidecar crate (`src-tauri/traffic-capture/`),
//! never in this process. `code_bridge` here only resolves and spawns that sidecar, reads its JSON
//! lines, and answers the six bridge actions the page calls. See docs/TRAFFIC/README.md for the
//! privilege-separation rationale (modeled on Wireshark's own GUI/`dumpcap` split) and the per-OS
//! permission steps.
//!
//! What this file guarantees, whatever the page does:
//! - Capture never starts implicitly and never without a NATIVE confirmation naming the interface
//!   and the filter (`start`). The page's token only proves who is asking, not that the person agreed.
//! - The sidecar lives exactly as long as its stdin: `stop` closes it (the sidecar finishes the file
//!   and reports why it stopped), a dead app closes it too, and the page being replaced or the app
//!   exiting stops the capture and deletes the file (`release`).
//! - Packets leave the sidecar only as the table rows the page renders and as the temporary file
//!   `export` copies, through a native save dialog, to where the person chose. Nothing is sent
//!   anywhere else.
//! - The capture file is private (0600) in the app's own cache folder, bounded in size by the
//!   sidecar, and stale ones from a crashed run are removed at the next start.
//!
//! Errors reach the page as `E_CODE: text` strings (the convention `pentest.js` and
//! `ipv4-search.js` use); the page translates the codes it knows and never prints the free text.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{Manager, WebviewWindow};

const PRAZO_SONDA: Duration = Duration::from_secs(10);
/// How long the sidecar has to open the device and say so, after the person agreed.
const PRAZO_ABERTURA: Duration = Duration::from_secs(5);
/// How long a sidecar asked to stop (stdin closed) has to finish the file before it is killed.
const PRAZO_PARADA: Duration = Duration::from_secs(3);
const UNAVAILABLE: &str =
    "E_CAPTURE_UNAVAILABLE: the traffic-capture engine was not found in this build";

/// What the sidecar has written to disk that is a whole number of packet blocks. Export copies
/// exactly this many bytes, so a capture still running is never exported cut in the middle of a
/// packet.
type SafeLen = Arc<AtomicU64>;

/// Where the page-facing events go. The app's emitter evals them into the window; a test's collects
/// them, which is what lets the whole Start → rows → Stop → Export flow run against the real
/// sidecar without a window.
type Emitter = Arc<dyn Fn(&'static str, Value) + Send + Sync>;

fn window_emitter(window: &WebviewWindow) -> Emitter {
    let window = window.clone();
    Arc::new(move |kind, event| emit(&window, kind, event))
}

enum Estado {
    Livre,
    /// The confirmation dialog is open, or the sidecar is opening the device.
    Iniciando { window: String },
    Capturando(Captura),
    /// Stopped by the person, by the size cap or by an error. The file stays for Export until the
    /// next start, the page being replaced, or the app exiting.
    Parada { window: String, temp: PathBuf, safe: SafeLen },
}

struct Captura {
    id: u64,
    window: String,
    child: Child,
    stdin: Option<ChildStdin>,
    temp: PathBuf,
    safe: SafeLen,
}

fn estado() -> &'static Mutex<Estado> {
    static ESTADO: OnceLock<Mutex<Estado>> = OnceLock::new();
    ESTADO.get_or_init(|| Mutex::new(Estado::Livre))
}

fn lock(slot: &Mutex<Estado>) -> std::sync::MutexGuard<'_, Estado> {
    slot.lock().unwrap_or_else(|e| e.into_inner())
}

/// Same shape as `motores::resolve_bin`'s step (1): a bundled `externalBin` always lands beside the
/// app's own executable. This sidecar is our own code, so there is no PATH-lookup fallback — only a
/// `tauri dev` convenience that looks at the sibling crate's own `target/release`.
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

/// `[{id, name, description, isUp, isLoopback}]` — the array `public/js/traffic.js` fills its
/// interface list from.
pub(super) fn interfaces() -> Result<Value, String> {
    let bin = resolve_sidecar().ok_or(UNAVAILABLE)?;
    let mut cmd = crate::processo::comando(bin);
    cmd.arg("--list-interfaces");
    let out = super::saida_com_prazo(cmd, PRAZO_SONDA)
        .map_err(|e| format!("E_CAPTURE_UNAVAILABLE: {e}"))?;
    let value = first_json_line(&out.stdout).ok_or(UNAVAILABLE)?;
    match value.get("interfaces") {
        Some(list) => Ok(list.clone()),
        None => Err(format!(
            "E_CAPTURE_UNAVAILABLE: {}",
            value.get("error").and_then(Value::as_str).unwrap_or("the capture engine did not list interfaces")
        )),
    }
}

/// `ready` / `needs_permission` / `missing_npcap` / `error` — the page never has to guess from an
/// exit code. `missing_npcap` only ever comes from the Windows branch: a sidecar that cannot even
/// start because the Npcap driver is absent typically fails at process load (a missing
/// `wpcap.dll`/`Packet.dll`), before it gets the chance to print anything of its own.
pub(super) fn permission_status(interface: Option<&str>) -> Value {
    let Some(bin) = resolve_sidecar() else {
        return json!({ "status": "error", "message": UNAVAILABLE });
    };
    let mut cmd = crate::processo::comando(&bin);
    cmd.arg("--check-permission");
    if let Some(i) = interface.filter(|i| valid_interface_id(i).is_ok()) {
        cmd.args(["--interface", i]);
    }
    match super::saida_com_prazo(cmd, PRAZO_SONDA) {
        Ok(out) => match first_json_line(&out.stdout) {
            Some(v) => v,
            None if cfg!(windows) => windows_missing_npcap(),
            None => json!({ "status": "error", "message": "the capture engine did not answer" }),
        },
        Err(_) if cfg!(windows) => windows_missing_npcap(),
        Err(e) => json!({ "status": "error", "message": e.to_string() }),
    }
}

fn windows_missing_npcap() -> Value {
    json!({
        "status": "missing_npcap",
        "message": "Npcap does not seem to be installed.",
        "url": "https://npcap.com/#download",
    })
}

/// The one explicit, user-initiated action that may trigger an OS elevation prompt — never run on
/// its own, never blanket "always allow" (same posture as `aparelho.rs`, ADR-039). It answers with
/// what the permission probe says AFTER the attempt, never with what the attempt hoped for.
pub(super) fn request_permission() -> Result<Value, String> {
    if resolve_sidecar().is_none() {
        return Err(UNAVAILABLE.into());
    }
    let attempt = grant_capture_capability();
    let after = permission_status(None);
    let granted = after.get("status").and_then(Value::as_str) == Some("ready");
    let mut answer = json!({ "granted": granted, "status": after.get("status").cloned().unwrap_or(Value::Null) });
    let note = match (&attempt, granted) {
        (_, true) => None,
        (Err(e), false) => Some(e.clone()),
        (Ok(()), false) => after.get("message").and_then(Value::as_str).map(str::to_string),
    };
    if let Some(note) = note {
        answer["message"] = json!(note);
    }
    if let Some(url) = after.get("url").cloned() {
        answer["url"] = url;
    }
    Ok(answer)
}

/// Linux: give the SIDECAR BINARY — not the app — the two capabilities libpcap needs, readable and
/// runnable only by the person's own group, with one polkit prompt. `chown`/`chmod` come first
/// because either one would wipe a capability set earlier. The paths travel as positional
/// parameters of a fixed script, never spliced into it.
#[cfg(target_os = "linux")]
fn grant_capture_capability() -> Result<(), String> {
    let bin = resolve_sidecar().ok_or(UNAVAILABLE)?;
    let bin = std::fs::canonicalize(bin).map_err(|e| e.to_string())?;
    if !bin.is_file() {
        return Err("the capture engine is not a regular file".into());
    }
    // SAFETY: getgid never fails and touches no memory.
    let gid = unsafe { libc::getgid() };
    let mut cmd = crate::processo::comando("pkexec");
    cmd.args([
        "/bin/sh",
        "-c",
        "chown root:\"$2\" \"$1\" && chmod 0750 \"$1\" && setcap cap_net_raw,cap_net_admin+eip \"$1\"",
        "sh",
    ])
    .arg(&bin)
    .arg(gid.to_string());
    match super::saida_com_prazo(cmd, Duration::from_secs(120)) {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
        Err(e) => Err(e.to_string()),
    }
}

/// macOS gives a process BPF access only through read access to `/dev/bpf*`, which a stock system
/// reserves for root. The usual, widely used fix is Wireshark's ChmodBPF helper (a launch daemon that
/// opens those devices to the `access_bpf` group the installer also creates). This app does not run
/// an unverified privileged command to imitate it: it says so and points there.
#[cfg(target_os = "macos")]
fn grant_capture_capability() -> Result<(), String> {
    Err("macOS only lets a member of a group with access to /dev/bpf* capture. Install the ChmodBPF \
         helper that comes with Wireshark (https://www.wireshark.org/download.html), then try again."
        .into())
}

/// Windows has no self-elevation path here: Npcap is installed by its own official installer,
/// which is the one that asks for administrator — never this app.
#[cfg(target_os = "windows")]
fn grant_capture_capability() -> Result<(), String> {
    Err("Install Npcap (https://npcap.com/#download), then try again.".into())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn grant_capture_capability() -> Result<(), String> {
    Err("this platform has no capture permission flow".into())
}

/// The interface id comes from a page. It is one argv element (`Command::arg`, never a shell), and
/// the sidecar checks it against the devices libpcap lists, so this only refuses what could never be
/// a device name.
fn valid_interface_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 256 || id.starts_with('-') || id.chars().any(char::is_control) {
        return Err(format!("E_INTERFACE_UNAVAILABLE: {id:?} is not an interface name"));
    }
    Ok(())
}

/// A capture filter is one argv element too. libpcap compiles it; a bad one comes back as
/// `E_CAPTURE_FILTER_INVALID` from the sidecar's own answer.
fn valid_filter(filter: &str) -> Result<(), String> {
    if filter.len() > 1024 || filter.contains('\0') {
        return Err("E_CAPTURE_FILTER_INVALID: the capture filter is too long or malformed".into());
    }
    Ok(())
}

/// Page-controlled text shown in a native dialog: control characters out, length bounded, quoted —
/// so it can neither add lines that read as the app's own words nor push the buttons off screen.
fn quoted(text: &str, max: usize) -> String {
    let clean: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let clean = clean.trim();
    if clean.chars().count() > max {
        format!("“{}…”", clean.chars().take(max).collect::<String>())
    } else {
        format!("“{clean}”")
    }
}

pub(super) fn confirmation_text(interface: &str, filter: &str) -> String {
    let filter = if filter.trim().is_empty() { "nenhum (todo o tráfego da interface)".to_string() } else { quoted(filter, 200) };
    format!(
        "Capturar os pacotes que passam pela interface {} deste computador?\n\n\
         Filtro de captura: {}\n\n\
         Os pacotes ficam num arquivo temporário, legível só por você, neste computador, até você \
         exportá-los ou descartá-los. O ShvIA não os envia a lugar nenhum. Uma captura pode incluir \
         dados privados de quem usa esta rede, como senhas enviadas sem criptografia.",
        quoted(interface, 80),
        filter,
    )
}

/// Blocks until answered, so it only runs off the UI thread (`fora_da_ui`): `blocking_show` on the
/// event loop that has to paint the dialog would deadlock.
fn confirm(window: &WebviewWindow, interface: &str, filter: &str) -> bool {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    window
        .app_handle()
        .dialog()
        .message(confirmation_text(interface, filter))
        .title("Autorizar captura de pacotes")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom("Capturar".into(), "Recusar".into()))
        .blocking_show()
}

/// The folder captures live in: the app's own cache, private to the person (0700).
fn capture_dir(window: &WebviewWindow) -> Result<PathBuf, String> {
    let base = window
        .app_handle()
        .path()
        .app_cache_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("traffic");
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&base).map_err(|e| format!("E_CAPTURE_FAILED: could not prepare {}: {e}", base.display()))?;
    Ok(base)
}

/// Removes every capture file in `dir`: only called when no capture is active, so what is there is
/// left over from a run that crashed.
fn remove_stale_captures(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "pcapng") {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn next_id() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Takes the one capture slot for `window`: refuses while another capture is running or starting,
/// and discards the file of a stopped one (a new capture replaces it).
fn reserve(slot: &Mutex<Estado>, window: &str) -> Result<(), String> {
    let mut guard = lock(slot);
    match &*guard {
        Estado::Capturando(_) | Estado::Iniciando { .. } => {
            Err("E_CAPTURE_BUSY: a capture is already running in this app".into())
        }
        Estado::Parada { temp, .. } => {
            let _ = std::fs::remove_file(temp);
            *guard = Estado::Iniciando { window: window.to_string() };
            Ok(())
        }
        Estado::Livre => {
            *guard = Estado::Iniciando { window: window.to_string() };
            Ok(())
        }
    }
}

/// Gives the slot back if it is still the one this window reserved.
fn unreserve(slot: &Mutex<Estado>, window: &str) {
    let mut guard = lock(slot);
    if matches!(&*guard, Estado::Iniciando { window: w } if w == window) {
        *guard = Estado::Livre;
    }
}

pub(super) fn start(window: &WebviewWindow, v: &Value) -> Result<Value, String> {
    let interface = v.get("interfaceId").and_then(Value::as_str).unwrap_or("");
    valid_interface_id(interface)?;
    // The capture filter only ever becomes ONE argv element handed straight to the sidecar — there
    // is no path from here into a shell, and none from the page's separate, client-side DISPLAY
    // filter into this call at all.
    let filter = v.get("filter").and_then(Value::as_str).unwrap_or("").trim();
    valid_filter(filter)?;
    let bin = resolve_sidecar().ok_or(UNAVAILABLE)?;
    let label = window.label().to_string();

    reserve(estado(), &label)?;
    if !confirm(window, interface, filter) {
        unreserve(estado(), &label);
        return Err("E_CAPTURE_DECLINED: the person declined the capture".into());
    }
    // A new document while the dialog was open already gave the slot back (`release`): the page that
    // asked is gone, so the answer starts nothing.
    if !matches!(&*lock(estado()), Estado::Iniciando { window: w } if *w == label) {
        return Err("E_CAPTURE_WINDOW_CHANGED: the page that asked for the capture is gone".into());
    }

    let started = capture_dir(window).and_then(|dir| {
        open_capture(estado(), window_emitter(window), &bin, &dir, interface, filter, &label)
    });
    if started.is_err() {
        unreserve(estado(), &label);
    }
    started
}

/// Spawns the sidecar and waits for it to say the device is open (or why not).
fn open_capture(
    slot: &'static Mutex<Estado>,
    emitter: Emitter,
    bin: &Path,
    dir: &Path,
    interface: &str,
    filter: &str,
    label: &str,
) -> Result<Value, String> {
    remove_stale_captures(dir);
    let id = next_id();
    let temp = dir.join(format!("capture-{}-{id}.pcapng", std::process::id()));

    let mut cmd = crate::processo::comando(bin);
    cmd.args(["--capture", "--interface", interface, "--out"]).arg(&temp);
    if !filter.is_empty() {
        cmd.args(["--filter", filter]);
    }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = cmd.spawn().map_err(|e| format!("E_CAPTURE_FAILED: {e}"))?;
    let (Some(stdout), Some(stdin)) = (child.stdout.take(), child.stdin.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("E_CAPTURE_FAILED: no pipes to the capture engine".into());
    };

    let safe: SafeLen = Arc::new(AtomicU64::new(0));
    let (tx, rx) = mpsc::channel::<Value>();
    {
        let safe = safe.clone();
        std::thread::spawn(move || read_loop(slot, emitter, stdout, id, safe, tx));
    }

    let first = rx.recv_timeout(PRAZO_ABERTURA);
    match first.as_ref().map(classify_first_line) {
        Ok(Handshake::Started) => {
            let mut guard = lock(slot);
            // The page may have been replaced in the instant since the dialog closed.
            if !matches!(&*guard, Estado::Iniciando { window: w } if w == label) {
                drop(guard);
                drop(stdin);
                let _ = child.wait();
                let _ = std::fs::remove_file(&temp);
                return Err("E_CAPTURE_WINDOW_CHANGED: the page that asked for the capture is gone".into());
            }
            *guard = Estado::Capturando(Captura {
                id,
                window: label.to_string(),
                child,
                stdin: Some(stdin),
                temp,
                safe,
            });
            Ok(json!({ "started": true }))
        }
        other => {
            let reason = match other {
                Ok(Handshake::Refused(error)) => error,
                _ => "E_CAPTURE_FAILED: the capture engine did not start".to_string(),
            };
            drop(stdin);
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(&temp);
            Err(reason)
        }
    }
}

enum Handshake {
    Started,
    /// The sidecar's refusal, already in the page's `E_CODE: text` shape.
    Refused(String),
}

fn classify_first_line(line: &Value) -> Handshake {
    if line.get("status").and_then(Value::as_str) == Some("started") {
        return Handshake::Started;
    }
    let text = line
        .get("error")
        .or_else(|| line.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("the capture engine refused to start");
    let code = line.get("code").and_then(Value::as_str).filter(|c| c.starts_with("E_")).unwrap_or(
        if line.get("status").and_then(Value::as_str) == Some("needs_permission") {
            "E_CAPTURE_PERMISSION_DENIED"
        } else {
            "E_CAPTURE_FAILED"
        },
    );
    Handshake::Refused(format!("{code}: {text}"))
}

/// Reads the sidecar's lines until it closes stdout. The FIRST line is the handshake and goes to
/// `start`; the rest become events for the page. When the sidecar ends on its own (size cap, error,
/// crash) the capture moves to "stopped" here, so the file stays exportable.
fn read_loop(
    slot: &'static Mutex<Estado>,
    emitter: Emitter,
    stdout: impl Read,
    id: u64,
    safe: SafeLen,
    handshake: mpsc::Sender<Value>,
) {
    let mut handshake = Some(handshake);
    let mut said_stopped = false;
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else { continue };
        if let Some(n) = value.get("flushed").or_else(|| value.get("bytes")).and_then(Value::as_u64) {
            safe.fetch_max(n, Ordering::Relaxed);
        }
        if let Some(tx) = handshake.take() {
            let _ = tx.send(value);
            continue;
        }
        said_stopped |= value.get("status").and_then(Value::as_str) == Some("stopped");
        if let Some((kind, event)) = event_for(&value) {
            emitter(kind, event);
        }
    }
    finished(slot, &emitter, id, said_stopped);
}

/// The page-facing event a sidecar line becomes (`None` for lines the page has no use for).
fn event_for(line: &Value) -> Option<(&'static str, Value)> {
    if line.get("packets").is_some() {
        return Some(("traffic_packet", line.clone()));
    }
    if line.get("error").is_some() {
        return Some(("traffic_error", line.clone()));
    }
    match line.get("status").and_then(Value::as_str) {
        Some("stopped") => Some(("traffic_status", json!({ "state": "stopped", "reason": line["reason"], "total": line["total"], "kernelDropped": line["kernelDropped"] }))),
        Some("needs_permission") | Some("error") => Some(("traffic_error", line.clone())),
        _ => None,
    }
}

fn emit(window: &WebviewWindow, kind: &str, event: Value) {
    let js = format!(
        "window.__shviaCode&&window.__shviaCode._emit(Object.assign(JSON.parse({}),{{type:{}}}))",
        super::js_str(&event.to_string()),
        super::js_str(kind),
    );
    let w = window.clone();
    let _ = window.app_handle().run_on_main_thread(move || {
        let _ = w.eval(&js);
    });
}

/// The sidecar closed its stdout. If `stop` already took the capture over, nothing is left to do;
/// if not, it ended by itself and the capture becomes "stopped" with its file kept.
fn finished(slot: &Mutex<Estado>, emitter: &Emitter, id: u64, said_stopped: bool) {
    let mut guard = lock(slot);
    let Estado::Capturando(c) = &mut *guard else { return };
    if c.id != id {
        return;
    }
    let _ = c.child.wait();
    let (label, temp, safe) = (c.window.clone(), c.temp.clone(), c.safe.clone());
    *guard = Estado::Parada { window: label, temp, safe };
    drop(guard);
    if !said_stopped {
        // Killed or crashed: there was no closing line, so the page is told here.
        emitter("traffic_status", json!({ "state": "stopped", "reason": "engine_exited" }));
    }
}

/// `trafficStop`: the person pressed Stop. Closing stdin is the polite request — the sidecar
/// finishes the file and says why it stopped — and the kill only follows a sidecar that does not
/// answer in time. The file stays for Export.
pub(super) fn stop(window: &str) -> Value {
    stop_in(estado(), Some(window), false)
}

/// The page was replaced or the app is exiting (`None` = every capture): stop and delete the file.
/// A new document has nobody left to export it.
pub(crate) fn release(window: Option<&str>) {
    stop_in(estado(), window, true);
}

fn stop_in(slot: &Mutex<Estado>, window: Option<&str>, discard: bool) -> Value {
    let mut guard = lock(slot);
    let mine = |label: &str| window.is_none_or(|w| w == label);
    let mut stopped = false;
    match std::mem::replace(&mut *guard, Estado::Livre) {
        Estado::Capturando(mut c) if mine(&c.window) => {
            drop(c.stdin.take());
            end_child(&mut c.child);
            stopped = true;
            *guard = Estado::Parada { window: c.window, temp: c.temp, safe: c.safe };
        }
        other => *guard = other,
    }
    // The dialog may be open for this page: dropping the reservation makes the answer a no-op.
    if matches!(&*guard, Estado::Iniciando { window: w } if mine(w)) {
        *guard = Estado::Livre;
    }
    if discard {
        if let Estado::Parada { window: w, temp, .. } = &*guard {
            if mine(w) {
                let _ = std::fs::remove_file(temp);
                *guard = Estado::Livre;
            }
        }
    }
    json!({ "stopped": stopped })
}

fn end_child(child: &mut Child) {
    let started = Instant::now();
    while started.elapsed() < PRAZO_PARADA {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// The file Export copies for `window`, with how many bytes of it are whole packet blocks.
fn export_source(slot: &Mutex<Estado>, window: &str) -> Option<(PathBuf, u64)> {
    match &*lock(slot) {
        Estado::Capturando(c) if c.window == window => Some((c.temp.clone(), c.safe.load(Ordering::Relaxed))),
        Estado::Parada { window: w, temp, safe } if w == window => Some((temp.clone(), safe.load(Ordering::Relaxed))),
        _ => None,
    }
}

/// Copies the first `len` bytes of `from` to `to`.
fn copy_prefix(from: &Path, to: &Path, len: u64) -> std::io::Result<u64> {
    let mut source = std::fs::File::open(from)?.take(len);
    let mut target = std::fs::File::create(to)?;
    let n = std::io::copy(&mut source, &mut target)?;
    target.flush()?;
    Ok(n)
}

pub(super) fn export(window: &WebviewWindow, req: String) {
    use tauri_plugin_dialog::DialogExt;

    let Some((temp, len)) = export_source(estado(), window.label()).filter(|(_, len)| *len > 0) else {
        super::reply(window, &req, false, json!("E_CAPTURE_NOTHING: nothing has been captured yet"));
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
                super::reply(&win, &req, true, json!({ "exported": false }));
                return;
            };
            let result = path
                .into_path()
                .map_err(|e| e.to_string())
                .and_then(|p| copy_prefix(&temp, &p, len).map(|_| p).map_err(|e| e.to_string()));
            match result {
                Ok(p) => super::reply(&win, &req, true, json!({ "exported": true, "path": p.to_string_lossy() })),
                Err(e) => super::reply(&win, &req, false, json!(format!("E_CAPTURE_EXPORT: {e}"))),
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_sidecar_never_panics_without_a_bundled_or_dev_binary() {
        // On a CI/dev machine with no staged sidecar the call must return `None` cleanly, so
        // `interfaces()`/`start()` can turn it into a normal `Err(UNAVAILABLE)` the page can show.
        let _ = resolve_sidecar();
    }

    #[test]
    fn an_interface_id_or_filter_from_a_page_is_checked_before_it_becomes_an_argument() {
        assert!(valid_interface_id("eth0").is_ok());
        assert!(valid_interface_id("\\Device\\NPF_{3F6A}").is_ok());
        for bad in ["", "--out", "-i", "eth0\nx", &"x".repeat(257)] {
            assert!(valid_interface_id(bad).unwrap_err().starts_with("E_INTERFACE_UNAVAILABLE: "), "{bad:?}");
        }
        assert!(valid_filter("tcp port 443; rm -rf /").is_ok(), "shell characters are inert in one argv element");
        assert!(valid_filter("a\0b").is_err());
        assert!(valid_filter(&"x".repeat(1025)).is_err());
    }

    #[test]
    fn the_confirmation_names_what_will_be_captured_and_cannot_be_dressed_up_by_the_page() {
        let text = confirmation_text("eth0", "tcp port 443");
        assert!(text.contains("“eth0”") && text.contains("“tcp port 443”"), "{text}");
        assert!(confirmation_text("eth0", "").contains("nenhum"));
        let spoof = confirmation_text("eth0\n\nO ShvIA garante que é seguro", "x\r\ny");
        assert!(!spoof.contains("eth0\n\n"), "a newline from the page must not start a line of the app's own text");
        let long = confirmation_text("e", &"a".repeat(5000));
        assert!(long.len() < 1200, "{}", long.len());
    }

    #[test]
    fn the_sidecars_first_line_is_a_start_or_a_refusal_in_the_pages_own_codes() {
        assert!(matches!(classify_first_line(&json!({"status":"started"})), Handshake::Started));
        let Handshake::Refused(denied) =
            classify_first_line(&json!({"status":"needs_permission","message":"socket: Operation not permitted"}))
        else {
            panic!("a permission refusal is not a start")
        };
        assert_eq!(denied, "E_CAPTURE_PERMISSION_DENIED: socket: Operation not permitted");
        let Handshake::Refused(filter) =
            classify_first_line(&json!({"error":"invalid capture filter","code":"E_CAPTURE_FILTER_INVALID"}))
        else {
            panic!()
        };
        assert!(filter.starts_with("E_CAPTURE_FILTER_INVALID: "));
        let Handshake::Refused(odd) = classify_first_line(&json!({"error":"x","code":"not a code"})) else { panic!() };
        assert!(odd.starts_with("E_CAPTURE_FAILED: "), "a code that is not E_… is never forwarded: {odd}");
    }

    #[test]
    fn sidecar_lines_become_the_events_the_page_listens_for() {
        let rows = json!({"packets":[{"id":"1"}],"total":1,"notShown":0});
        assert_eq!(event_for(&rows).map(|(k, _)| k), Some("traffic_packet"));
        assert_eq!(event_for(&json!({"error":"x","code":"E_CAPTURE_WRITE"})).map(|(k, _)| k), Some("traffic_error"));
        let stopped = json!({"status":"stopped","reason":"size_limit","total":9,"bytes":5,"kernelDropped":0});
        let (kind, event) = event_for(&stopped).unwrap();
        assert_eq!((kind, event["state"].as_str(), event["reason"].as_str()), ("traffic_status", Some("stopped"), Some("size_limit")));
        assert!(event_for(&json!({"status":"started"})).is_none());
    }

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("shvia-traffic-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, b"0123456789").unwrap();
        path
    }

    #[test]
    fn one_capture_at_a_time_and_a_stopped_ones_file_is_replaced_by_the_next() {
        let slot = Mutex::new(Estado::Livre);
        reserve(&slot, "main").unwrap();
        assert!(reserve(&slot, "other").unwrap_err().starts_with("E_CAPTURE_BUSY: "));
        unreserve(&slot, "other");
        assert!(matches!(&*lock(&slot), Estado::Iniciando { .. }), "another window cannot give back a reservation that is not its own");
        unreserve(&slot, "main");
        assert!(matches!(&*lock(&slot), Estado::Livre));

        let old = temp_file("old.pcapng");
        *lock(&slot) = Estado::Parada { window: "main".into(), temp: old.clone(), safe: Arc::new(AtomicU64::new(10)) };
        reserve(&slot, "main").unwrap();
        assert!(!old.exists(), "starting a new capture discards the previous one's file");
    }

    #[test]
    fn replacing_a_page_stops_its_capture_and_deletes_its_file_but_not_another_pages() {
        let slot = Mutex::new(Estado::Livre);
        let file = temp_file("page.pcapng");
        *lock(&slot) = Estado::Parada { window: "a".into(), temp: file.clone(), safe: Arc::new(AtomicU64::new(10)) };
        stop_in(&slot, Some("b"), true);
        assert!(file.exists() && matches!(&*lock(&slot), Estado::Parada { .. }), "window b has no say over a's capture");
        stop_in(&slot, Some("a"), true);
        assert!(!file.exists() && matches!(&*lock(&slot), Estado::Livre));

        *lock(&slot) = Estado::Iniciando { window: "a".into() };
        stop_in(&slot, Some("a"), true);
        assert!(matches!(&*lock(&slot), Estado::Livre), "a dialog open for a replaced page gives its slot back");
    }

    #[test]
    fn stop_with_nothing_running_reports_not_stopped() {
        let slot = Mutex::new(Estado::Livre);
        assert_eq!(stop_in(&slot, Some("some-window"), false), json!({ "stopped": false }));
    }

    #[test]
    fn export_copies_only_the_whole_blocks_and_only_for_the_window_that_captured() {
        let slot = Mutex::new(Estado::Livre);
        let file = temp_file("export.pcapng");
        *lock(&slot) = Estado::Parada { window: "a".into(), temp: file.clone(), safe: Arc::new(AtomicU64::new(7)) };
        assert!(export_source(&slot, "b").is_none());
        let (path, len) = export_source(&slot, "a").unwrap();
        let out = file.with_file_name("exported.pcapng");
        assert_eq!(copy_prefix(&path, &out, len).unwrap(), 7);
        assert_eq!(std::fs::read(&out).unwrap(), b"0123456");
    }

    // ── against the REAL sidecar and REAL packets ─────────────────────────────────────────────
    // Opening a device needs a capture capability, so these run inside a throw-away user + network
    // namespace (root there owns CAP_NET_RAW over that namespace only; no sudo, no real interface
    // touched): `scripts/traffic-e2e.sh`. Plain `cargo test` lists them as ignored.

    fn real_sidecar() -> PathBuf {
        resolve_sidecar().expect("build the sidecar first: cargo build --release --manifest-path src-tauri/traffic-capture/Cargo.toml")
    }

    fn collecting_emitter() -> (Emitter, mpsc::Receiver<(&'static str, Value)>) {
        let (tx, rx) = mpsc::channel();
        let emitter: Emitter = Arc::new(move |kind, event| {
            let _ = tx.send((kind, event));
        });
        (emitter, rx)
    }

    fn leaked_slot() -> &'static Mutex<Estado> {
        Box::leak(Box::new(Mutex::new(Estado::Livre)))
    }

    fn private_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("shvia-traffic-e2e-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The block types of a pcapng file (little-endian hosts): 0x0A0D0D0A section, 1 interface, 6 packet.
    fn pcapng_blocks(bytes: &[u8]) -> Vec<u32> {
        let (mut at, mut kinds) = (0usize, Vec::new());
        while at + 8 <= bytes.len() {
            let kind = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
            let len = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
            kinds.push(kind);
            if len < 12 {
                break;
            }
            at += len;
        }
        kinds
    }

    fn send_udp(datagrams: usize) {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        for i in 0..datagrams {
            socket.send_to(format!("hello-{i}").as_bytes(), "127.0.0.1:9999").unwrap();
        }
    }

    /// Waits for an event of `kind` that `accept` likes, collecting what comes meanwhile.
    fn wait_for(
        rx: &mpsc::Receiver<(&'static str, Value)>,
        seen: &mut Vec<(&'static str, Value)>,
        kind: &str,
        accept: impl Fn(&Value) -> bool,
    ) -> Value {
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(found) = seen.iter().find(|(k, v)| *k == kind && accept(v)) {
                return found.1.clone();
            }
            let left = until.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(event) => seen.push(event),
                Err(_) => panic!("no {kind} event arrived in time; saw {seen:?}"),
            }
        }
    }

    #[test]
    #[ignore = "needs a capture capability: run scripts/traffic-e2e.sh (a throw-away network namespace)"]
    fn a_real_capture_from_start_to_export_and_release() {
        let (bin, slot, dir) = (real_sidecar(), leaked_slot(), private_dir("flow"));
        let (emitter, rx) = collecting_emitter();

        // A refused start is an error at Start, with a code the page knows — not "capturing" forever.
        reserve(slot, "w").unwrap();
        let bad_filter = open_capture(slot, emitter.clone(), &bin, &dir, "lo", "tcp port (", "w").unwrap_err();
        assert!(bad_filter.starts_with("E_CAPTURE_FILTER_INVALID: "), "{bad_filter}");
        let no_device = open_capture(slot, emitter.clone(), &bin, &dir, "no-such-nic0", "", "w").unwrap_err();
        assert!(no_device.starts_with("E_INTERFACE_UNAVAILABLE: "), "{no_device}");
        assert!(std::fs::read_dir(&dir).unwrap().next().is_none(), "a refused start leaves no file behind");

        let started = open_capture(slot, emitter, &bin, &dir, "lo", "udp port 9999", "w").unwrap();
        assert_eq!(started, json!({ "started": true }));
        assert!(matches!(&*lock(slot), Estado::Capturando(_)));

        send_udp(5);
        let mut seen = Vec::new();
        let rows = wait_for(&rx, &mut seen, "traffic_packet", |v| v["total"].as_u64() == Some(5));
        let first = &rows["packets"][0];
        assert_eq!(first["protocol"], "UDP");
        assert_eq!(first["dst"], "127.0.0.1");
        assert!(first["hex"].as_str().unwrap().contains("hello-"), "the hex dump carries the payload: {first}");
        assert!(first["summary"].as_str().unwrap().contains("-> :9999"));

        let (temp, safe) = export_source(slot, "w").expect("a running capture can be exported");
        assert!(safe > 0 && safe <= std::fs::metadata(&temp).unwrap().len(), "only whole blocks are offered: {safe}");

        assert_eq!(stop_in(slot, Some("w"), false), json!({ "stopped": true }));
        assert!(matches!(&*lock(slot), Estado::Parada { .. }), "Stop keeps the file for Export");
        let stopped = wait_for(&rx, &mut seen, "traffic_status", |v| v["state"] == "stopped");
        assert_eq!(stopped["reason"], "requested");
        assert_eq!(stopped["total"], 5);

        let (temp, safe) = export_source(slot, "w").expect("a stopped capture can still be exported");
        let exported = dir.join("exported.pcapng");
        copy_prefix(&temp, &exported, safe).unwrap();
        let bytes = std::fs::read(&exported).unwrap();
        let blocks = pcapng_blocks(&bytes);
        assert_eq!(blocks[0], 0x0A0D_0D0A, "section header first");
        assert_eq!(blocks[1], 1, "then the interface description (without it the file is invalid)");
        assert_eq!(blocks.iter().filter(|b| **b == 6).count(), 5, "all five packets");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&temp).unwrap().permissions().mode() & 0o777, 0o600);
        }

        stop_in(slot, Some("w"), true); // what `release` does for a replaced page
        assert!(!temp.exists(), "the page being replaced deletes the capture");
        assert!(matches!(&*lock(slot), Estado::Livre));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore = "needs a capture capability: run scripts/traffic-e2e.sh (a throw-away network namespace)"]
    fn a_sidecar_that_dies_leaves_a_stopped_capture_with_its_file_and_tells_the_page() {
        let (bin, slot, dir) = (real_sidecar(), leaked_slot(), private_dir("crash"));
        let (emitter, rx) = collecting_emitter();
        reserve(slot, "w").unwrap();
        open_capture(slot, emitter, &bin, &dir, "lo", "udp port 9999", "w").unwrap();
        send_udp(3);
        let mut seen = Vec::new();
        wait_for(&rx, &mut seen, "traffic_packet", |v| v["total"].as_u64() == Some(3));

        // The engine is killed from outside, with no chance to say goodbye.
        match &mut *lock(slot) {
            Estado::Capturando(c) => c.child.kill().unwrap(),
            _ => panic!("not capturing"),
        }
        let told = wait_for(&rx, &mut seen, "traffic_status", |v| v["state"] == "stopped");
        assert_eq!(told["reason"], "engine_exited");
        let (temp, safe) = export_source(slot, "w").expect("what was written stays exportable");
        assert!(safe > 0 && temp.exists());
        stop_in(slot, None, true); // what `release` does when the app exits
        assert!(!temp.exists(), "leaving the app deletes it");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_captures_are_removed_and_other_files_are_left_alone() {
        let dir = std::env::temp_dir().join(format!("shvia-traffic-stale-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("capture-1-1.pcapng"), b"x").unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        remove_stale_captures(&dir);
        assert!(!dir.join("capture-1-1.pcapng").exists());
        assert!(dir.join("notes.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
