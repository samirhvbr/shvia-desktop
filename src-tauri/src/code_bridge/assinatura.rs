//! Connecting the person's Claude subscription to ShvIA from this machine (1.28.0, `recursos.assinatura`).
//!
//! `claude setup-token` makes the one-year token the Workspace runs Claude Code on. Until now the person ran it
//! in a terminal and pasted what it printed into Settings → Engines. This module does both steps: it runs the
//! client under a pseudo-terminal, reads the token off the screen the client draws, and hands it to the ShvIA
//! server the shell is configured for. The token is never in the page, in an event, in a log or on a command
//! line (ADR-043).
//!
//! ## What was read, and what was not run
//!
//! The installed client (2.1.300) was read, not run: its `setup-token` action renders an Ink screen
//! (`ConsoleOAuthFlow`, mode `setup-token`), starts the browser sign-in at once, offers the sign-in URL and a
//! paste prompt for a code, and on success draws
//!
//! ```text
//! ✓ Long-lived authentication token created successfully!
//! Your OAuth token (valid for 1 year):
//! sk-ant-oat01-…
//! Store this token securely. You won't be able to see it again.
//! ```
//!
//! then leaves with code 0. An Ink screen wants a terminal (raw keyboard input for the paste prompt), which is
//! why [`pty`] gives it one. The sign-in itself needs the person's browser, so **no test here can run it**: the
//! parsing is proved on fixtures of that screen, the process handling on stand-ins that draw it, and the whole
//! on the person's machine by the runbook in `docs/code/ASSINATURA-NO-WORKSPACE.md`.
//!
//! ## Where the token goes, and why not through the page
//!
//! Straight to the server, by the shell, with the panel's access token the page hands over. The page never sees the token
//! (nothing in the server's own origin, an injected script included, can read it back), the flow finishes even
//! if the person navigates away while the browser is open, and the destination is the configured server — not
//! an address the page names.

#[cfg(unix)]
mod pty;
#[cfg(unix)]
mod servidor;
#[cfg(unix)]
mod tela;

use super::*;
#[cfg(unix)]
use std::sync::atomic::{AtomicU64, Ordering as Ordem};
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
/// How long a flow may take before it is given up: a sign-in with a second browser, the account chooser and a
/// two-factor prompt fit; a client left waiting all afternoon does not. A choice, like the login's ceiling.
pub(super) const TETO_DA_ASSINATURA: Duration = Duration::from_secs(15 * 60);

/// Why a flow did not finish, as a code the page maps to its own sentence, plus a Portuguese fallback for a page
/// that does not know the code. Nothing in `mensagem` comes from the client's screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Falha {
    pub(super) codigo: &'static str,
    pub(super) mensagem: String,
}

pub(super) fn falha(codigo: &'static str, mensagem: impl Into<String>) -> Falha {
    Falha { codigo, mensagem: mensagem.into() }
}

#[cfg(unix)]
/// A step of the flow, as the page is told. The sign-in URL is the one piece of the screen that is passed on, and
/// only after a host check (`tela::achar_url`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Evento {
    Aguardando { url: Option<String> },
    Enviando,
    Conectado,
    Falhou(Falha),
    Cancelado,
}

#[cfg(unix)]
/// How a flow ended, for whoever drove it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Desfecho {
    Conectado,
    Falhou(&'static str),
    Cancelado,
}

#[cfg(unix)]
/// What the page receives (`window.__shviaCode.onEvent`). Built here and nowhere else, from a closed set of
/// fields: no output of the client, no token, no session.
pub(super) fn para_json(fluxo: u64, evento: &Evento) -> serde_json::Value {
    let mut v = serde_json::json!({ "type": "claude_assinatura", "fluxo": fluxo });
    let obj = v.as_object_mut().expect("an object literal");
    match evento {
        Evento::Aguardando { url } => {
            obj.insert("fase".into(), "aguardando".into());
            if let Some(u) = url {
                obj.insert("url".into(), u.clone().into());
            }
        }
        Evento::Enviando => {
            obj.insert("fase".into(), "enviando".into());
        }
        Evento::Conectado => {
            obj.insert("fase".into(), "conectado".into());
        }
        Evento::Falhou(f) => {
            obj.insert("fase".into(), "falhou".into());
            obj.insert("codigo".into(), f.codigo.into());
            obj.insert("mensagem".into(), f.mensagem.clone().into());
        }
        Evento::Cancelado => {
            obj.insert("fase".into(), "cancelado".into());
        }
    }
    v
}

#[cfg(unix)]
/// The text of the native confirmation. It names the profile and the server: the decision is which account's
/// token goes to which server, and a dialog that does not say is a click-through.
pub(super) fn texto_da_confirmacao(perfil: &str, servidor: &str) -> String {
    let perfil = if perfil.trim().is_empty() { "sem nome" } else { perfil.trim() };
    format!(
        "Conectar a assinatura do Claude ao ShvIA?\n\n\
         O Claude Code (perfil “{perfil}”) vai abrir o navegador para você autorizar a conta do Claude que \
         quer conectar. O token de um ano que ele gera vai direto para {servidor}, sem aparecer na tela, \
         e passa a ser usado pelo Claude Code dos seus projetos do Workspace. Se já houver uma assinatura \
         conectada, esta a substitui.\n\n\
         Continue só se foi você quem pediu isto agora.",
    )
}

#[cfg(unix)]
/// The number of the flow now running, 0 when none; the page tells a stale event from a current one by it.
static GERACAO: AtomicU64 = AtomicU64::new(0);

#[cfg(unix)]
pub(super) fn proxima_geracao() -> u64 {
    GERACAO.fetch_add(1, Ordem::SeqCst) + 1
}

// ── unix: the real thing ────────────────────────────────────────────────────────────────────────

#[cfg(unix)]
struct Ativo {
    geracao: u64,
    controle: std::sync::Arc<pty::Controle>,
}

#[cfg(unix)]
static EM_CURSO: OnceLock<Mutex<Option<Ativo>>> = OnceLock::new();

#[cfg(unix)]
fn slot() -> &'static Mutex<Option<Ativo>> {
    EM_CURSO.get_or_init(|| Mutex::new(None))
}

/// Cancels the flow in progress, if any. Idempotent, and called when the app quits: without it a `claude` waiting
/// for a sign-in nobody is going to finish would outlive the window that asked for it.
#[cfg(unix)]
pub fn cancelar_assinatura() {
    if let Ok(mut g) = slot().lock() {
        if let Some(a) = g.take() {
            a.controle.cancelar();
        }
    }
}

#[cfg(unix)]
fn liberar_slot(geracao: u64) {
    if let Ok(mut g) = slot().lock() {
        if g.as_ref().is_some_and(|a| a.geracao == geracao) {
            *g = None;
        }
    }
}

/// Tells the page, on the thread that owns its webview.
#[cfg(unix)]
fn emitir_para_pagina(window: &WebviewWindow, fluxo: u64, evento: &Evento) {
    let js = format!("window.__shviaCode&&window.__shviaCode._emit({})", para_json(fluxo, evento));
    let app = window.app_handle().clone();
    let w = window.clone();
    let _ = app.run_on_main_thread(move || {
        let _ = w.eval(&js);
    });
}

/// Starts a flow and returns its number at once; what happens next arrives as events. A flow already running is
/// cancelled first: two sign-ins at once would leave the person authorising the wrong one.
#[cfg(unix)]
pub(super) fn iniciar_assinatura(
    window: &WebviewWindow,
    conta: Option<&crate::contas_claude::Alvo>,
    acesso_do_painel: &str,
) -> Result<u64, Falha> {
    cancelar_assinatura();
    let servidor = servidor::servidor_configurado()?;
    let acesso = servidor::validar_acesso(acesso_do_painel)?;
    let bin = resolve_bin("claude").ok_or_else(|| {
        falha("cli_ausente", "O Claude Code não está instalado neste computador (não achei o comando `claude`).")
    })?;
    let origem: Vec<(String, String)> = std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .collect();
    let cmd = pty::montar_comando(&bin, &["setup-token"], conta, &origem, crate::user_env::sidecar_path());
    let aberta = pty::abrir_sessao(cmd)?;

    let geracao = proxima_geracao();
    if let Ok(mut g) = slot().lock() {
        *g = Some(Ativo { geracao, controle: aberta.controle.clone() });
    }
    let w = window.clone();
    std::thread::spawn(move || {
        let emitir = |e: Evento| emitir_para_pagina(&w, geracao, &e);
        let pausas = [Duration::from_secs(1), Duration::from_secs(3)];
        let entregar = |token: &tela::Segredo| servidor::entregar(&servidor, &acesso, token, &pausas);
        let _ = pty::conduzir(aberta, TETO_DA_ASSINATURA, entregar, emitir);
        liberar_slot(geracao);
    });
    Ok(geracao)
}

/// Types the code the browser showed into the client's terminal. Checked for shape here; written and forgotten.
#[cfg(unix)]
pub(super) fn entregar_codigo_da_assinatura(codigo: &str) -> Result<(), Falha> {
    let c = codigo.trim();
    if c.is_empty() || c.len() > 600 || c.chars().any(|ch| ch.is_control()) {
        return Err(falha("codigo_invalido", "Esse código não parece o que o navegador mostrou. Copie-o inteiro."));
    }
    let controle = slot()
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|a| a.controle.clone()))
        .ok_or_else(|| falha("sem_fluxo", "Não há conexão em andamento. Comece de novo."))?;
    controle.digitar(c)
}

/// The server the token would go to, checked BEFORE the person is asked: a confirmation for a flow that cannot
/// end is a question that should not have been put.
#[cfg(unix)]
pub(super) fn servidor_do_envio() -> Result<String, Falha> {
    servidor::servidor_configurado().map(|s| s.base)
}

/// Asks the person, in a native dialog, before anything is started. Blocks until answered, so only off the UI
/// thread (the dialog needs the event loop that a blocked UI thread would be holding).
#[cfg(unix)]
pub(super) fn confirmar_assinatura(window: &WebviewWindow, perfil: &str, servidor: &str) -> bool {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    window
        .app_handle()
        .dialog()
        .message(texto_da_confirmacao(perfil, servidor))
        .title("Conectar o Claude ao ShvIA")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom("Conectar".into(), "Cancelar".into()))
        .blocking_show()
}

// ── other systems: the capability is absent, and says why ───────────────────────────────────────

#[cfg(not(unix))]
pub fn cancelar_assinatura() {}

#[cfg(not(unix))]
pub(super) fn iniciar_assinatura(
    _window: &WebviewWindow,
    _conta: Option<&crate::contas_claude::Alvo>,
    _acesso_do_painel: &str,
) -> Result<u64, Falha> {
    Err(falha("nao_suportado", "Conectar por aqui ainda não está disponível neste sistema. Cole o token em Configurações → Motores."))
}

#[cfg(not(unix))]
pub(super) fn entregar_codigo_da_assinatura(_codigo: &str) -> Result<(), Falha> {
    Err(falha("nao_suportado", "Conectar por aqui ainda não está disponível neste sistema."))
}

#[cfg(not(unix))]
pub(super) fn servidor_do_envio() -> Result<String, Falha> {
    Err(falha("nao_suportado", "Conectar por aqui ainda não está disponível neste sistema."))
}

#[cfg(not(unix))]
pub(super) fn confirmar_assinatura(_window: &WebviewWindow, _perfil: &str, _servidor: &str) -> bool {
    false
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_confirmacao_nomeia_o_perfil_e_o_servidor() {
        let t = texto_da_confirmacao("Empresa Blue3", "https://ai.shvia.org");
        assert!(t.contains("“Empresa Blue3”") && t.contains("https://ai.shvia.org"), "{t}");
        assert!(t.contains("substitui"), "diz que troca a assinatura já conectada: {t}");
        assert!(texto_da_confirmacao("  ", "x").contains("“sem nome”"));
    }

    /// The event is built from a closed set of fields. A token, a session or a line of the client's screen has no
    /// field to travel in.
    #[test]
    fn o_evento_tem_so_os_campos_da_lista() {
        let v = para_json(7, &Evento::Aguardando { url: Some("https://claude.com/oauth/authorize?x=1".into()) });
        assert_eq!(v["type"], "claude_assinatura");
        assert_eq!(v["fluxo"], 7);
        assert_eq!(v["fase"], "aguardando");
        assert_eq!(v["url"], "https://claude.com/oauth/authorize?x=1");
        assert_eq!(para_json(7, &Evento::Aguardando { url: None }).get("url"), None);
        for (ev, fase) in [(Evento::Enviando, "enviando"), (Evento::Conectado, "conectado"), (Evento::Cancelado, "cancelado")] {
            let j = para_json(1, &ev);
            assert_eq!(j["fase"], fase);
            assert_eq!(j.as_object().unwrap().len(), 3, "{j}");
        }
        let f = para_json(2, &Evento::Falhou(falha("sem_token", "sem token")));
        let mut chaves: Vec<_> = f.as_object().unwrap().keys().map(String::as_str).collect();
        chaves.sort_unstable();
        assert_eq!(chaves, ["codigo", "fase", "fluxo", "mensagem", "type"]);
    }

    /// SOURCE check, declared as such: a dialog needs a display, so no unit test can click it. It guards the order the
    /// feature depends on, inside the `claudeSubscriptionConnect` arm: off the UI thread first, then the server is
    /// checked, then the native confirmation, then (and only then) the client is started — with the refusal
    /// returning in between. The needles are built at run time so this test's own text never matches them.
    #[test]
    fn a_conexao_so_sobe_depois_do_dialogo_e_fora_da_thread_da_interface() {
        let fonte = include_str!("../code_bridge.rs");
        let ini = fonte.find(&["pub fn handle", "_message("].concat()).expect("handle_message moved");
        let corpo = &fonte[ini..];
        let chave = ["\"claudeSubscription", "Connect\" =>"].concat();
        let a = corpo.find(&chave).expect("claudeSubscriptionConnect arm not found");
        let resto = &corpo[a + chave.len()..];
        let braco = &resto[..resto.find("\n        \"").unwrap_or(resto.len())];
        let pos = |agulha: &str| braco.find(agulha).unwrap_or_else(|| panic!("`{agulha}` is not in the arm:\n{braco}"));
        let despacho = pos(&["fora_da", "_ui("].concat());
        let servidor = pos(&["servidor_do", "_envio("].concat());
        let dialogo = pos(&["confirmar", "_assinatura(w"].concat());
        let recusa = pos(&["\"cance", "lado\""].concat());
        let inicio = pos(&["iniciar", "_assinatura(w"].concat());
        assert!(despacho < servidor && servidor < dialogo, "the dialog blocks, and the server is checked before it asks");
        assert!(dialogo < recusa && recusa < inicio, "the client is started before the person says yes");
    }

    /// The page asks the capability, and the shim answers it per system: true where the shell can give the client a
    /// terminal. No placeholder survives the injection.
    #[test]
    fn o_shim_declara_a_assinatura_e_os_tres_metodos() {
        let js = crate::code_bridge::inject_token(crate::code_bridge::BRIDGE_JS);
        assert!(!js.contains("__SHVIA_"), "um placeholder sobrou no shim injetado");
        assert!(js.contains("assinatura: true"), "unix: a capacidade está declarada");
        for metodo in ["claudeSubscriptionConnect", "claudeSubscriptionCode", "claudeSubscriptionCancel"] {
            assert!(js.contains(&format!("{metodo}: function")), "{metodo}");
        }
        // The token of the panel is the only secret a call carries in, and the method never takes a token OUT.
        assert!(js.contains("the token NEVER reaches the page"));
    }

    /// SOURCE check, declared as such: nothing in this module's code may print. A `println!` or a `format!` of the
    /// token is the one mistake that cannot be undone, and ausência is not provable by running the happy path.
    /// The needles are built at run time so this test's own text never matches them.
    #[test]
    fn nada_neste_modulo_imprime_ou_registra() {
        let proibidos = [
            ["print", "ln!"].concat(),
            ["eprint", "ln!"].concat(),
            ["db", "g!"].concat(),
            ["log", "::"].concat(),
            ["trac", "ing::"].concat(),
            ["{", ":?}"].concat(),
        ];
        let fontes = [
            ("assinatura.rs", include_str!("assinatura.rs")),
            ("pty.rs", include_str!("assinatura/pty.rs")),
            ("tela.rs", include_str!("assinatura/tela.rs")),
            ("servidor.rs", include_str!("assinatura/servidor.rs")),
        ];
        for (nome, fonte) in fontes {
            // Only the code: the tests below `mod tests` and the comments may say anything.
            let codigo = fonte.split("#[cfg(test)]").next().unwrap_or("");
            for (i, linha) in codigo.lines().enumerate() {
                let t = linha.trim_start();
                if t.starts_with("//") {
                    continue;
                }
                for p in &proibidos {
                    // Whole words: `log::` must not match the end of `dialog::`.
                    let achou = linha.match_indices(p.as_str()).any(|(pos, _)| {
                        !linha[..pos].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_')
                    });
                    assert!(!achou, "{nome}:{}: `{p}` no caminho do token: {linha}", i + 1);
                }
            }
        }
    }
}
