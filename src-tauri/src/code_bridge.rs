//! Ponte nativa do **Modo Code** (F2). Expõe ao ShvIA web (página remota) o que
//! só o desktop pode fazer: spawnar o `anna --json` como **sidecar** na pasta do
//! projeto, conversar por stdin/stdout (NDJSON), escolher pasta e guardar o
//! vínculo projeto→pasta.
//!
//! Postura de menor privilégio (ADR-001): **não** usa comando/IPC Tauri. O canal
//! page→Rust é o **script-message-handler do WebKit** (`window.webkit
//! .messageHandlers.shviaCode`, igual à ponte de TTS); Rust→page é `eval`. O
//! transporte está no `configure_linux_webview` (WebKitGTK); a lógica aqui é
//! agnóstica de SO — macOS/Windows entram registrando o handler deles e chamando
//! `handle_message`. Sem handler (ex.: mobile / SO ainda sem ponte) o shim não
//! define `window.__shviaDesktop`, então o web esconde o Modo Code (fail-safe).
//!
//! Protocolo NDJSON do `anna`: `SHVIA-CODE/docs/embedding.md`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;
use tauri::{Manager, WebviewWindow};

/// O shim injetado em cada página (`on_page_load`). Define `window.__shviaCode`
/// (a API que a UI do Modo Code no SHVIA-WEB chama) e `window.__shviaDesktop`
/// (flag p/ o web mostrar o Modo Code SÓ onde a ponte existe). Autocontido, ES5,
/// self-guard: sem o handler nativo, não faz nada.
pub const BRIDGE_JS: &str = r#"(function () {
  if (window.__shviaCode) return;
  var mh = window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.shviaCode;
  if (!mh) return; // sem ponte nativa → sem Modo Code (fail-safe)
  var reqs = {}, seq = 0, listeners = [];
  function post(action, data) {
    return new Promise(function (res, rej) {
      var id = 'r' + (++seq);
      reqs[id] = { res: res, rej: rej };
      var msg = { action: action, reqId: id };
      if (data) for (var k in data) msg[k] = data[k];
      mh.postMessage(JSON.stringify(msg));
    });
  }
  window.__shviaCode = {
    // sessão do agente
    spawn: function (o) { return post('spawn', o || {}); },   // {projectDir, apiKey, model?, effort?, url?}
    send:  function (o) { return post('send', { payload: o }); }, // {type:'user',text} | {id,decision}
    kill:  function () { return post('kill'); },
    onEvent: function (cb) { if (typeof cb === 'function') listeners.push(cb); },
    // pasta / vínculo
    pickFolder: function () { return post('pickFolder'); },
    getBinding: function (pid) { return post('getBinding', { projectId: pid }); },
    setBinding: function (pid, path) { return post('setBinding', { projectId: pid, path: path }); },
    // chamados pelo Rust (eval):
    _reply: function (id, ok, data) { var r = reqs[id]; if (r) { delete reqs[id]; ok ? r.res(data) : r.rej(data); } },
    _emit: function (evt) { for (var i = 0; i < listeners.length; i++) { try { listeners[i](evt); } catch (e) {} } }
  };
  window.__shviaDesktop = { platform: 'linux', bridge: 'webkit' };
})();"#;

/// Um sidecar `anna` de uma janela (uma sessão code ativa por janela).
struct Sidecar {
    child: Child,
    stdin: ChildStdin,
}

/// Registro de sidecars por rótulo de janela — em Tauri managed state.
#[derive(Default)]
pub struct Sidecars(Mutex<HashMap<String, Sidecar>>);

impl Sidecars {
    fn kill_label(&self, label: &str) {
        if let Ok(mut map) = self.0.lock() {
            if let Some(mut sc) = map.remove(label) {
                let _ = sc.child.kill();
                let _ = sc.child.wait();
            }
        }
    }

    /// Mata o sidecar de UMA janela (fechou a janela).
    pub fn kill_one(&self, label: &str) {
        self.kill_label(label);
    }

    /// Mata todos (saída do app — anti-órfão).
    pub fn kill_all(&self) {
        if let Ok(mut map) = self.0.lock() {
            for (_, mut sc) in map.drain() {
                let _ = sc.child.kill();
                let _ = sc.child.wait();
            }
        }
    }

    /// Registra o sidecar da janela, matando um anterior (troca de sessão).
    fn insert(&self, label: String, child: Child, stdin: ChildStdin) {
        if let Ok(mut map) = self.0.lock() {
            if let Some(mut old) = map.insert(label, Sidecar { child, stdin }) {
                let _ = old.child.kill();
                let _ = old.child.wait();
            }
        }
    }

    /// Escreve uma linha no stdin do sidecar da janela (mensagem ou decisão).
    fn send_line(&self, label: &str, line: &str) {
        if let Ok(mut map) = self.0.lock() {
            if let Some(sc) = map.get_mut(label) {
                let _ = writeln!(sc.stdin, "{line}");
                let _ = sc.stdin.flush();
            }
        }
    }
}

/// Localiza o binário `anna`: PATH primeiro, senão `~/.local/bin/anna` (onde o
/// `install.sh` do SHVIA-CODE o coloca).
fn resolve_anna() -> Option<PathBuf> {
    if let Ok(out) = Command::new("sh").arg("-c").arg("command -v anna").output() {
        if out.status.success() {
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !p.is_empty() {
                return Some(PathBuf::from(p));
            }
        }
    }
    let home = std::env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".local/bin/anna");
    p.exists().then_some(p)
}

/// Ponto de entrada do handler nativo: recebe uma mensagem JSON da página.
pub fn handle_message(window: &WebviewWindow, payload: &str) {
    let v: serde_json::Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(_) => return,
    };
    let action = v.get("action").and_then(|a| a.as_str()).unwrap_or_default();
    let req = v.get("reqId").and_then(|r| r.as_str()).unwrap_or_default().to_string();
    match action {
        "spawn" => spawn(window, &req, &v),
        "send" => {
            send(window, &v);
            reply(window, &req, true, serde_json::json!({ "ok": true }));
        }
        "kill" => {
            window.app_handle().state::<Sidecars>().kill_label(window.label());
            reply(window, &req, true, serde_json::json!({ "ok": true }));
        }
        "pickFolder" => pick_folder(window, req),
        "getBinding" => {
            let pid = v.get("projectId").and_then(|p| p.as_str()).unwrap_or_default();
            let path = load_bindings(window).get(pid).and_then(|x| x.as_str()).map(String::from);
            reply(window, &req, true, serde_json::json!({ "path": path }));
        }
        "setBinding" => {
            set_binding(window, &v);
            reply(window, &req, true, serde_json::json!({ "ok": true }));
        }
        _ => reply(window, &req, false, serde_json::json!({ "error": "ação desconhecida" })),
    }
}

fn spawn(window: &WebviewWindow, req: &str, v: &serde_json::Value) {
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_string();
    let dir = s("projectDir");
    if dir.is_empty() || !PathBuf::from(&dir).is_dir() {
        return reply(window, req, false, serde_json::json!({ "error": "pasta do projeto inválida" }));
    }
    let Some(bin) = resolve_anna() else {
        return reply(window, req, false,
            serde_json::json!({ "error": "anna não encontrado — instale com o install.sh do SHVIA-CODE" }));
    };

    let mut cmd = Command::new(bin);
    cmd.arg("--json").args(["--tools", "local"]).current_dir(&dir);
    let (model, effort, url, key) = (s("model"), s("effort"), s("url"), s("apiKey"));
    if !model.is_empty() {
        cmd.args(["--model", &model]);
    }
    if !effort.is_empty() {
        cmd.args(["--effort", &effort]);
    }
    if !url.is_empty() {
        cmd.args(["--url", &url]);
    }
    if !key.is_empty() {
        cmd.env("SHVIA_API_KEY", &key); // chave do usuário, repassada pelo web (nunca logada)
    }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return reply(window, req, false, serde_json::json!({ "error": format!("falha ao iniciar anna: {e}") })),
    };
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");
    let stdin = child.stdin.take().expect("stdin piped");

    // Guarda no state, matando um sidecar anterior desta janela (troca de sessão).
    let label = window.label().to_string();
    window.app_handle().state::<Sidecars>().insert(label, child, stdin);

    // stderr → log do app (nunca a timeline).
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            eprintln!("[anna] {line}");
        }
    });

    // stdout NDJSON → evento na página (`_emit`), no main thread (WebKit).
    let win = window.clone();
    std::thread::spawn(move || {
        let app = win.app_handle().clone();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            let js = format!("window.__shviaCode&&window.__shviaCode._emit(JSON.parse({}))", js_str(&line));
            let w = win.clone();
            let _ = app.run_on_main_thread(move || {
                let _ = w.eval(&js);
            });
        }
        // stdout fechou → anna saiu.
        let w = win.clone();
        let _ = app.run_on_main_thread(move || {
            let _ = w.eval("window.__shviaCode&&window.__shviaCode._emit({type:'exited'})");
        });
    });

    reply(window, req, true, serde_json::json!({ "ok": true }));
}

fn send(window: &WebviewWindow, v: &serde_json::Value) {
    let line = match v.get("payload") {
        Some(p) => serde_json::to_string(p).unwrap_or_default(),
        None => return,
    };
    window.app_handle().state::<Sidecars>().send_line(window.label(), &line);
}

fn pick_folder(window: &WebviewWindow, req: String) {
    use tauri_plugin_dialog::DialogExt;
    let win = window.clone();
    window.app_handle().dialog().file().pick_folder(move |path| {
        let data = serde_json::json!({ "path": path.map(|p| p.to_string()) });
        reply(&win, &req, true, data);
    });
}

// ── vínculo projeto→pasta (config local do dispositivo) ─────────────────────

fn bindings_path(window: &WebviewWindow) -> Option<PathBuf> {
    let dir = window.app_handle().path().app_config_dir().ok()?;
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("modo-code-bindings.json"))
}

fn load_bindings(window: &WebviewWindow) -> serde_json::Map<String, serde_json::Value> {
    bindings_path(window)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn set_binding(window: &WebviewWindow, v: &serde_json::Value) {
    let pid = v.get("projectId").and_then(|x| x.as_str()).unwrap_or_default();
    let path = v.get("path").and_then(|x| x.as_str()).unwrap_or_default();
    if pid.is_empty() {
        return;
    }
    let mut map = load_bindings(window);
    map.insert(pid.to_string(), serde_json::json!(path));
    if let (Some(p), Ok(s)) = (bindings_path(window), serde_json::to_string_pretty(&map)) {
        let _ = std::fs::write(p, s);
    }
}

// ── helpers ─────────────────────────────────────────────────────────────────

/// Responde uma requisição da página (`_reply`), no main thread.
fn reply(window: &WebviewWindow, req: &str, ok: bool, data: serde_json::Value) {
    let js = format!(
        "window.__shviaCode&&window.__shviaCode._reply({}, {}, {})",
        js_str(req),
        ok,
        data
    );
    let w = window.clone();
    let app = window.app_handle().clone();
    let _ = app.run_on_main_thread(move || {
        let _ = w.eval(&js);
    });
}

/// String → literal JS seguro (escapa aspas/controle; U+2028/2029 são válidos em
/// JSON mas quebram string literal JS).
fn js_str(s: &str) -> String {
    serde_json::to_string(s)
        .unwrap_or_else(|_| "\"\"".into())
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}
