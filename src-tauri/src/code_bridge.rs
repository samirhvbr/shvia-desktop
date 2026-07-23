//! Ponte nativa do **Modo Code** (F2). Expõe ao ShvIA web (página remota) o que
//! só o desktop pode fazer: spawnar o `anna --json` como **sidecar** na pasta do
//! projeto, conversar por stdin/stdout (NDJSON), escolher pasta e guardar o
//! vínculo projeto→pasta.
//!
//! Postura de menor privilégio (ADR-001): **não** usa comando/IPC Tauri. O canal
//! page→Rust é o **message-handler nativo do WebView**, que muda por SO:
//! `window.webkit.messageHandlers.shviaCode` no WebKit (Linux/macOS) e
//! `window.chrome.webview` no WebView2 (Windows). Rust→page é sempre `eval`. O
//! transporte é registrado por SO — `configure_linux_webview` (WebKitGTK),
//! `macos_ipc::install` (WKWebView) e `windows_ipc::install` (WebView2) —, mas a
//! lógica aqui é agnóstica de SO: todos chamam `handle_message`. Sem handler
//! (ex.: mobile) o shim não define `window.__shviaDesktop`, então o web esconde
//! o Modo Code (fail-safe).
//!
//! Protocolo NDJSON do `anna`: `SHVIA-CODE/docs/embedding.md`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use tauri::{Manager, WebviewWindow};
use tauri_plugin_notification::NotificationExt;

/// Token de capacidade da sessão do app. É injetado SOMENTE nas páginas do host
/// canônico (`ia.blue3.com.br`, via `on_page_load`) dentro do shim da ponte, e
/// TODA mensagem página→Rust precisa carregá-lo em `__t`.
///
/// Fecha o furo do handler nativo ser alcançável por QUALQUER frame: o
/// `messageHandler`/`window.chrome.webview` existe para todos os frames da
/// webview, então um `<iframe>` cross-origin embutido na página poderia postar
/// `spawn`/`listTree`/`gitStatus` direto. Esse iframe NÃO consegue ler o token
/// (não injetado nele + closure do frame pai é cross-origin), então suas
/// mensagens são descartadas em `handle_message`. Uniforme nos 3 SOs — não
/// depende de API de origem de frame (que o webkit2gtk 2.0 não expõe).
static BRIDGE_TOKEN: OnceLock<String> = OnceLock::new();

/// Token da sessão (gerado uma vez por processo). 32 hex, seguro em JS/JSON.
pub fn bridge_token() -> &'static str {
    BRIDGE_TOKEN
        .get_or_init(|| uuid::Uuid::new_v4().simple().to_string())
        .as_str()
}

/// Interpola o token de sessão no placeholder `__SHVIA_BRIDGE_TOKEN__` de um
/// script injetado (BRIDGE_JS e PRICE_ALERT_NOTIFY_JS). Chamado no `on_page_load`.
pub fn inject_token(js: &str) -> String {
    js.replace("__SHVIA_BRIDGE_TOKEN__", bridge_token())
}

/// O shim injetado em cada página (`on_page_load`). Define `window.__shviaCode`
/// (a API que a UI do Modo Code no SHVIA-WEB chama) e `window.__shviaDesktop`
/// (flag p/ o web mostrar o Modo Code SÓ onde a ponte existe). Autocontido, ES5,
/// self-guard: sem o handler nativo, não faz nada.
pub const BRIDGE_JS: &str = r#"(function () {
  if (window.__shviaCode) return;
  // Transporte página→Rust por WebView, escolhido por SO:
  //  - WebKit (Linux/macOS): window.webkit.messageHandlers.shviaCode.postMessage(str)
  //  - WebView2 (Windows):   window.chrome.webview.postMessage(str)
  // Rust→página é sempre eval (_reply/_emit). Sem nenhum dos dois → sem Modo
  // Code (fail-safe: não define __shviaCode e o web esconde o toggle).
  var wk = window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.shviaCode;
  var w2 = window.chrome && window.chrome.webview;
  if (!wk && !w2) return;
  function sendNative(str) { if (wk) { wk.postMessage(str); } else { w2.postMessage(str); } }
  var reqs = {}, seq = 0, listeners = [];
  function post(action, data) {
    return new Promise(function (res, rej) {
      var id = 'r' + (++seq);
      reqs[id] = { res: res, rej: rej };
      var msg = { action: action, reqId: id };
      if (data) for (var k in data) msg[k] = data[k];
      msg.__t = '__SHVIA_BRIDGE_TOKEN__'; // token de capacidade (ver bridge_token)
      sendNative(JSON.stringify(msg));
    });
  }
  window.__shviaCode = {
    // sessão do agente
    spawn: function (o) { return post('spawn', o || {}); },   // {projectDir, apiKey, model?, effort?, url?, engine?}  engine:'claude' = motor assinatura (Claude Code)
    send:  function (o) { return post('send', { payload: o }); }, // {type:'user',text} | {id,decision}
    kill:  function () { return post('kill'); },
    onEvent: function (cb) { if (typeof cb === 'function') listeners.push(cb); },
    // pasta / vínculo
    pickFolder: function () { return post('pickFolder'); },
    getBinding: function (pid) { return post('getBinding', { projectId: pid }); },
    setBinding: function (pid, path) { return post('setBinding', { projectId: pid, path: path }); },
    // painel da pasta (read-only, pelo app)
    gitStatus: function (path) { return post('gitStatus', { path: path }); },
    listTree: function (path) { return post('listTree', { path: path }); },
    // chamados pelo Rust (eval):
    _reply: function (id, ok, data) { var r = reqs[id]; if (r) { delete reqs[id]; ok ? r.res(data) : r.rej(data); } },
    _emit: function (evt) { for (var i = 0; i < listeners.length; i++) { try { listeners[i](evt); } catch (e) {} } }
  };
  window.__shviaDesktop = wk ? { platform: 'webkit', bridge: 'webkit' }
                             : { platform: 'windows', bridge: 'webview2' };
})();"#;

/// Um sidecar `anna` de uma janela (uma sessão code ativa por janela). `gen` é a
/// geração da sessão — a thread de stdout usa pra saber se ainda é a sessão viva.
struct Sidecar {
    child: Child,
    stdin: ChildStdin,
    gen: u64,
}

/// Registro de sidecars por rótulo de janela — em Tauri managed state.
/// Cada entrada fica em `Option<Sidecar>` para que `send_line` consiga TIRAR
/// o sidecar do mapa sob o lock, escrever no stdin FORA do lock (o pipe pode
/// bloquear se o anna estiver ocupado), e devolver a entrada no lock de novo.
#[derive(Default)]
pub struct Sidecars {
    map: Mutex<HashMap<String, Option<Sidecar>>>,
    next_gen: AtomicU64,
}

impl Sidecars {
    fn kill_label(&self, label: &str) {
        // Remove sob o lock, mas mata/espera FORA dele — child.wait() pode bloquear
        // e seguraria o Mutex global (send/insert de outras janelas travariam).
        let sc = self
            .map
            .lock()
            .ok()
            .and_then(|mut m| m.remove(label))
            .and_then(|opt| opt);
        if let Some(mut sc) = sc {
            let _ = sc.child.kill();
            let _ = sc.child.wait();
        }
    }

    /// Mata o sidecar de UMA janela (fechou a janela).
    pub fn kill_one(&self, label: &str) {
        self.kill_label(label);
    }

    /// Mata todos (saída do app — anti-órfão).
    pub fn kill_all(&self) {
        let drained: Vec<Sidecar> = self
            .map
            .lock()
            .ok()
            .map(|mut m| {
                m.drain()
                    .filter_map(|(_, opt)| opt)
                    .collect()
            })
            .unwrap_or_default();
        for mut sc in drained {
            let _ = sc.child.kill();
            let _ = sc.child.wait();
        }
    }

    /// Registra o sidecar da janela (nova geração), matando o anterior (troca de
    /// sessão). Devolve a geração desta sessão.
    fn insert(&self, label: String, child: Child, stdin: ChildStdin) -> u64 {
        let gen = self.next_gen.fetch_add(1, Ordering::Relaxed) + 1;
        let old = self.map.lock().ok().and_then(|mut m| {
            m.insert(
                label,
                Some(Sidecar {
                    child,
                    stdin,
                    gen,
                }),
            )
        });
        if let Some(Some(mut old)) = old {
            let _ = old.child.kill();
            let _ = old.child.wait();
        }
        gen
    }

    /// A sessão viva desta janela ainda é a geração `gen`? (senão houve respawn/kill,
    /// e a thread de stdout velha NÃO deve sinalizar 'exited' pra não derrubar a nova).
    fn is_current(&self, label: &str, gen: u64) -> bool {
        self.map
            .lock()
            .map(|m| {
                m.get(label)
                    .and_then(|opt| opt.as_ref())
                    .map(|sc| sc.gen == gen)
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    /// Escreve uma linha no stdin do sidecar da janela (mensagem ou decisão).
    /// `false` = não havia sessão viva ou a escrita falhou (o host então sabe que
    /// a decisão/mensagem caiu, em vez de assumir ok).
    ///
    /// Importante: o `writeln!+flush` acontece FORA do Mutex global — se o pipe
    /// do anna encher (tool longa, gate bloqueante), só esta chamada trava;
    /// outras janelas e o spawn/kill continuam funcionando.
    fn send_line(&self, label: &str, line: &str) -> bool {
        let mut sidecar = match self.map.lock() {
            Ok(mut map) => match map.get_mut(label) {
                Some(slot) => slot.take(),
                None => None,
            },
            Err(_) => None,
        };
        let Some(mut sc) = sidecar.take() else {
            return false;
        };
        let ok = writeln!(sc.stdin, "{line}").and_then(|_| sc.stdin.flush()).is_ok();
        if let Ok(mut map) = self.map.lock() {
            if let Some(slot) = map.get_mut(label) {
                *slot = Some(sc);
            }
            // Se a janela foi removida entre os dois locks, recria sem gravar
            // de volta (a sessão está morta). Sem leak: o Child morre no drop.
        }
        ok
    }
}

/// Localiza um binário de motor (`anna` ou `claude-runner`), cross-platform.
/// Ordem: (1) ao lado do executável do ShvIA Desktop — permite empacotar o
/// binário como resource/sidecar do instalador; (2) no PATH; (3) locais
/// conhecidos por SO (`~/.local/bin/<base>` no Unix, `%LOCALAPPDATA%\Programs\
/// <base>\<base>.exe` no Windows).
fn resolve_bin(base: &str) -> Option<PathBuf> {
    let exe_name = if cfg!(windows) { format!("{base}.exe") } else { base.to_string() };

    // (1) Ao lado do próprio app (bundled).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join(&exe_name);
            if p.exists() {
                return Some(p);
            }
        }
    }

    // (2)+(3) por SO.
    #[cfg(windows)]
    {
        if let Ok(out) = Command::new("where").arg(base).output() {
            if out.status.success() {
                // `where` pode listar vários — pega a 1ª linha.
                if let Some(line) = String::from_utf8_lossy(&out.stdout).lines().next() {
                    let p = PathBuf::from(line.trim());
                    if p.exists() {
                        return Some(p);
                    }
                }
            }
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let p = PathBuf::from(local).join("Programs").join(base).join(&exe_name);
            if p.exists() {
                return Some(p);
            }
        }
    }

    #[cfg(not(windows))]
    {
        if let Ok(out) = Command::new("sh").arg("-c").arg(format!("command -v {base}")).output() {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() {
                    return Some(PathBuf::from(p));
                }
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            let p = PathBuf::from(home).join(".local/bin").join(base);
            if p.exists() {
                return Some(p);
            }
        }
    }

    None
}

/// Ponto de entrada do handler nativo: recebe uma mensagem JSON da página.
pub fn handle_message(window: &WebviewWindow, payload: &str) {
    let v: serde_json::Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(_) => return,
    };
    // Cap de capacidade: só a página injetada em ia.blue3.com.br conhece o token
    // de sessão. Um <iframe> cross-origin embutido alcança o messageHandler
    // nativo mas não tem o token → descartado (silêncio, sem oráculo). Cobre os
    // 3 SOs num só ponto, sem depender de API de origem de frame.
    if v.get("__t").and_then(|t| t.as_str()) != Some(bridge_token()) {
        return;
    }
    let action = v.get("action").and_then(|a| a.as_str()).unwrap_or_default();
    let req = v.get("reqId").and_then(|r| r.as_str()).unwrap_or_default().to_string();
    match action {
        "spawn" => spawn(window, &req, &v),
        "send" => {
            let ok = send(window, &v);
            reply(window, &req, ok, serde_json::json!({ "ok": ok }));
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
        "gitStatus" => {
            let path = v.get("path").and_then(|x| x.as_str()).unwrap_or_default();
            reply(window, &req, true, git_status(path));
        }
        "listTree" => {
            let path = v.get("path").and_then(|x| x.as_str()).unwrap_or_default();
            reply(window, &req, true, list_tree(path));
        }
        // Notificação nativa do SO (alertas de preço, ADR-011). Fire-and-forget:
        // sem reqId/reply — a página só dispara, não espera resposta.
        "notify" => notify(window, &v),
        _ => reply(window, &req, false, serde_json::json!({ "error": "ação desconhecida" })),
    }
}

fn spawn(window: &WebviewWindow, req: &str, v: &serde_json::Value) {
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_string();
    let dir = s("projectDir");
    if dir.is_empty() || !PathBuf::from(&dir).is_dir() {
        return reply(window, req, false, serde_json::json!({ "error": "pasta do projeto inválida" }));
    }
    // Motor: "gateway" (anna, default) ou "claude" (claude-runner — Claude Code
    // com a ASSINATURA do usuário, FORA do gateway). Ambos falam o mesmo NDJSON
    // (embedding.md), então o bridge e a UI de cards não mudam. Sem `engine` = anna.
    let engine = s("engine");
    let is_claude = engine == "claude";
    let exe_base = if is_claude { "claude-runner" } else { "anna" };
    let Some(bin) = resolve_bin(exe_base) else {
        let err = if is_claude {
            "claude-runner não encontrado — rode claude-runner/install.sh (deixa em ~/.local/bin) e faça `claude login` (usa a assinatura; sem API key)."
        } else {
            "anna não encontrado — instale o anna (SHVIA-CODE) e deixe no PATH (Unix: install.sh; Windows: anna.exe no PATH ou %LOCALAPPDATA%\\Programs\\anna)"
        };
        return reply(window, req, false, serde_json::json!({ "error": err }));
    };

    let mut cmd = Command::new(bin);
    cmd.current_dir(&dir);
    let (model, effort, url, key) = (s("model"), s("effort"), s("url"), s("apiKey"));
    if is_claude {
        // Assinatura via cliente oficial: passa só a pasta do projeto. NADA de
        // SHVIA_API_KEY / --url / --effort (não passa pelo gateway). O modelo é o
        // do login/assinatura; o perfil do gateway não se aplica aqui.
        cmd.args(["--cwd", &dir]);
    } else {
        cmd.arg("--json").args(["--tools", "local"]);
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
    // A geração desta sessão acompanha a thread de stdout (guarda o 'exited').
    let label = window.label().to_string();
    let gen = window.app_handle().state::<Sidecars>().insert(label.clone(), child, stdin);

    // stderr → log do app (nunca a timeline).
    let tag = exe_base.to_string();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            eprintln!("[{tag}] {line}");
        }
    });

    // stdout NDJSON → evento na página (`_emit`), no main thread (WebKit).
    let win = window.clone();
    let exit_label = label;
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
        // stdout fechou → anna saiu. Só sinaliza 'exited' se ESTA geração ainda é a
        // sessão viva da janela; se foi substituída (respawn) ou morta (kill/troca
        // de projeto), fica quieta pra não derrubar a sessão nova que acabou de subir.
        if app.state::<Sidecars>().is_current(&exit_label, gen) {
            let w = win.clone();
            let _ = app.run_on_main_thread(move || {
                let _ = w.eval("window.__shviaCode&&window.__shviaCode._emit({type:'exited'})");
            });
        }
    });

    reply(window, req, true, serde_json::json!({ "ok": true }));
}

fn send(window: &WebviewWindow, v: &serde_json::Value) -> bool {
    let line = match v.get("payload") {
        Some(p) => serde_json::to_string(p).unwrap_or_default(),
        None => return false,
    };
    window.app_handle().state::<Sidecars>().send_line(window.label(), &line)
}

/// Dispara uma notificação nativa do SO (alertas de preço, ADR-011). Usa só a API
/// Rust do `tauri-plugin-notification` — nenhuma capability é exposta à página
/// remota (mantém o ADR-001). Best-effort: falha (permissão do SO negada, etc.) é
/// silenciosa e nunca afeta a página. `handle_message` já roda na main/UI thread
/// em todos os SOs (WKScriptMessageHandler / WebKitGTK / WebView2), então é seguro
/// mostrar a notificação daqui.
fn notify(window: &WebviewWindow, v: &serde_json::Value) {
    let title = v.get("title").and_then(|x| x.as_str()).unwrap_or("ShvIA");
    let body = v.get("body").and_then(|x| x.as_str()).unwrap_or_default();
    if body.trim().is_empty() {
        return;
    }
    // Neutraliza markup: alguns daemons de notificação freedesktop (Linux)
    // interpretam <b>/<a href> no corpo. O conteúdo é first-party (nome do item
    // que o próprio usuário cadastrou), mas removemos os sinais por precaução.
    let sanitize = |s: &str| s.replace(['<', '>'], " ");
    let _ = window
        .app_handle()
        .notification()
        .builder()
        .title(sanitize(title))
        .body(sanitize(body))
        .show();
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

// ── painel da pasta: git status + árvore (read-only, pelo app — F4) ─────────

fn git_status(path: &str) -> serde_json::Value {
    if path.is_empty() {
        return serde_json::json!({ "repo": false });
    }
    match Command::new("git").args(["-C", path, "status", "--porcelain=v1", "-b"]).output() {
        Ok(o) if o.status.success() => parse_git_status(&String::from_utf8_lossy(&o.stdout)),
        _ => serde_json::json!({ "repo": false }), // não é repo git
    }
}

fn parse_git_status(text: &str) -> serde_json::Value {
    let mut branch = String::new();
    let mut files = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            branch = rest.split("...").next().unwrap_or("").split(' ').next().unwrap_or("").to_string();
        } else if line.len() > 3 {
            files.push(serde_json::json!({ "status": line[..2].trim(), "path": &line[3..] }));
        }
    }
    serde_json::json!({ "repo": true, "branch": branch, "files": files })
}

/// Um nível da árvore (lazy-load ao expandir). Ignora pastas de build/deps.
fn list_tree(path: &str) -> serde_json::Value {
    const IGNORE: &[&str] = &[".git", "node_modules", "vendor", "target", "dist", "build", ".svn"];
    let mut entries: Vec<(bool, String, String)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(path) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if IGNORE.contains(&name.as_str()) {
                continue;
            }
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            entries.push((is_dir, name, e.path().to_string_lossy().into_owned()));
        }
    }
    // pastas primeiro, depois alfabético.
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())));
    let arr: Vec<serde_json::Value> = entries
        .into_iter()
        .map(|(d, name, p)| serde_json::json!({ "name": name, "path": p, "isDir": d }))
        .collect();
    serde_json::json!({ "entries": arr })
}

// ── helpers ─────────────────────────────────────────────────────────────────

/// Responde uma requisição da página (`_reply`), no main thread.
fn reply(window: &WebviewWindow, req: &str, ok: bool, data: serde_json::Value) {
    // `data` vai como JSON.parse(<string>) — mesmo caminho seguro do stdout — pra
    // que js_str neutralize aspas e U+2028/2029 (um path/erro com esses chars
    // quebraria o eval se `data` fosse interpolado cru).
    let js = format!(
        "window.__shviaCode&&window.__shviaCode._reply({}, {}, JSON.parse({}))",
        js_str(req),
        ok,
        js_str(&data.to_string())
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
