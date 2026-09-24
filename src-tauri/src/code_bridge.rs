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

// 1.6.41: the bridge is split by subject. This file keeps the dispatch (`handle_message`), what
// every part shares (`reply`, `fora_da_ui`, `saida_com_prazo` and its deadlines), and the two
// small OS hooks the page calls (`notify`, `badge`).
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
mod sessao;
use sessao::*;
pub use sessao::Sidecars;
pub(crate) use motores::{engine_status, versao_do_anna};

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
