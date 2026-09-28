//! The `shvia://` scheme: a link that opens the app at the right place (1.10.0, ADR-038).
//!
//! ## What a link can do, and why it is a closed list
//!
//! Any web page, e-mail or chat message can carry a `shvia://` link, and one click hands it to
//! this process. So a link is **input from a stranger**, and what it reaches is decided here,
//! from a fixed list, never read from the link:
//!
//! | link | what happens |
//! |---|---|
//! | `shvia://` · `shvia://abrir` | brings the app forward |
//! | `shvia://rapida` | toggles the quick window (the way to bind a system shortcut on Wayland) |
//! | `shvia://chat` | a new window at the chat |
//! | `shvia://chat?q=texto` | a new window at the chat, with `texto` in the composer, **not sent** |
//! | `shvia://configuracoes/<secao>` | a new window with that section of the settings open |
//! | anything else | brings the app forward, and nothing else |
//!
//! **Nothing a link does runs anything.** The prompt lands in the composer (SHVIA-WEB's
//! `/chat?q=`, which never sends), so the person reads it and presses send, or does not. That is
//! why there is no confirmation dialog, unlike OpenClaw's `openclaw://agent`, which RUNS the
//! message and has to ask first (`DeepLinks.swift`). A route that would send, approve or run
//! something does not belong in this list without that confirmation.
//!
//! The link never chooses the server or the path: the window goes to the server the owner
//! configured (`server.rs`), at a path built here. The shell repeats the check on the path it is
//! given (`destino()` in `src/main.ts`), so it never becomes a redirect to another origin.

use tauri::Url;

pub const ESQUEMA: &str = "shvia";

/// Longest prompt a link may put in the composer, in characters. A link is typed or pasted by
/// someone, and 8 000 characters is several screens of text; past it, it is a payload, not a
/// question, and the link is refused whole — never cut, since a cut prompt says something else.
pub const MAX_PROMPT: usize = 8_000;

/// Where a link sends the app. See the table in the module docs.
#[derive(Debug, PartialEq, Eq)]
pub enum Rota {
    Abrir,
    Rapida,
    /// A path on the configured server, always one `destino_permitido` accepts.
    Destino(String),
    /// Not a link of ours, or one this version does not know. The app is brought forward, and
    /// the link is not logged beyond its scheme and host (it may carry someone's text).
    Recusada,
}

/// Reads a link. Pure, so every route is tested without an app.
pub fn rota(link: &str) -> Rota {
    let Ok(url) = Url::parse(link.trim()) else {
        return Rota::Recusada;
    };
    if !url.scheme().eq_ignore_ascii_case(ESQUEMA) {
        return Rota::Recusada;
    }
    // A non-special scheme keeps the host as typed: `shvia://Chat` is the same link.
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let segmentos: Vec<&str> = url.path().split('/').filter(|s| !s.is_empty()).collect();

    match host.as_str() {
        "" | "abrir" | "open" => Rota::Abrir,
        "rapida" | "quick" => Rota::Rapida,
        "chat" => {
            let q = url.query_pairs().find(|(k, _)| k == "q").map(|(_, v)| v.into_owned());
            match q.as_deref().map(str::trim) {
                None | Some("") => Rota::Destino("/chat".into()),
                Some(t) if t.chars().count() > MAX_PROMPT => Rota::Recusada,
                Some(t) => Rota::Destino(com_parametro("q", t)),
            }
        }
        "configuracoes" | "settings" => match segmentos.as_slice() {
            [secao] if secao_valida(secao) => Rota::Destino(com_parametro("configuracoes", secao)),
            _ => Rota::Recusada,
        },
        _ => Rota::Recusada,
    }
}

/// A settings section is a short slug (`github`, `perfil`, `contas-claude`). SHVIA-WEB opens its
/// Profile pane for an unknown one, so a wrong slug is harmless — the check is about what may
/// travel into the URL, not about which sections exist.
fn secao_valida(s: &str) -> bool {
    (1..=40).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// `/chat?<nome>=<valor>`, encoded by the URL parser itself (form encoding: SHVIA-WEB reads it
/// with `URLSearchParams`, which decodes `+` as a space).
fn com_parametro(nome: &str, valor: &str) -> String {
    let mut u = Url::parse("http://x/chat").expect("fixed URL");
    u.query_pairs_mut().append_pair(nome, valor);
    format!("{}?{}", u.path(), u.query().unwrap_or_default())
}

/// The paths the shell may be sent to. The SAME rule as `destino()` in `src/main.ts`, and a
/// test below keeps the two written alike: this side decides, that side refuses anything else.
pub fn destino_permitido(caminho: &str) -> bool {
    caminho == "/chat" || caminho.starts_with("/chat?")
}

/// The local shell's address for a destination: it probes the server, then goes there.
pub fn caminho_da_casca(destino: &str) -> String {
    let mut u = Url::parse("http://x/index.html").expect("fixed URL");
    u.query_pairs_mut().append_pair("ir", destino);
    format!("index.html?{}", u.query().unwrap_or_default())
}

/// Does this command-line argument look like a link of ours? The single-instance callback asks,
/// so a link does not ALSO raise the main window (the quick window would hide behind it).
pub fn e_link(arg: &str) -> bool {
    arg.get(..ESQUEMA.len() + 1)
        .is_some_and(|p| p.eq_ignore_ascii_case(&format!("{ESQUEMA}:")))
}

/// Shortest gap between two windows opened by links.
pub const INTERVALO: std::time::Duration = std::time::Duration::from_secs(2);

static ULTIMA_JANELA: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// May a link open a window now? Records the time when it says yes.
pub fn pode_abrir_janela() -> bool {
    let Ok(mut ultima) = ULTIMA_JANELA.lock() else { return false };
    let agora = std::time::Instant::now();
    if !passou_o_intervalo(*ultima, agora) {
        return false;
    }
    *ultima = Some(agora);
    true
}

fn passou_o_intervalo(ultima: Option<std::time::Instant>, agora: std::time::Instant) -> bool {
    ultima.is_none_or(|u| agora.duration_since(u) >= INTERVALO)
}

/// What may be logged about a refused link: scheme and host, never the query (it can carry the
/// text someone meant to ask).
pub fn para_o_log(link: &str) -> String {
    match Url::parse(link.trim()) {
        Ok(u) => format!("{}://{}", u.scheme(), u.host_str().unwrap_or_default()),
        Err(_) => "(não é URL)".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_link_and_abrir_bring_the_app_forward() {
        assert_eq!(rota("shvia://"), Rota::Abrir);
        assert_eq!(rota("shvia://abrir"), Rota::Abrir);
        assert_eq!(rota("SHVIA://Abrir/"), Rota::Abrir);
    }

    #[test]
    fn rapida_toggles_the_quick_window() {
        assert_eq!(rota("shvia://rapida"), Rota::Rapida);
        assert_eq!(rota("shvia://quick"), Rota::Rapida);
    }

    #[test]
    fn chat_with_a_prompt_fills_the_composer_encoded() {
        assert_eq!(rota("shvia://chat"), Rota::Destino("/chat".into()));
        assert_eq!(rota("shvia://chat?q="), Rota::Destino("/chat".into()));
        assert_eq!(
            rota("shvia://chat?q=resuma%20o%20contrato%20%26%20os%20prazos"),
            Rota::Destino("/chat?q=resuma+o+contrato+%26+os+prazos".into()),
            "the `&` stays inside the prompt: it cannot open a second parameter"
        );
        // Another parameter in the link does not travel: only `q` is read.
        assert_eq!(rota("shvia://chat?q=oi&enviar=1"), Rota::Destino("/chat?q=oi".into()));
    }

    #[test]
    fn a_prompt_past_the_limit_is_refused_whole_never_cut() {
        let no_limite = "a".repeat(MAX_PROMPT);
        assert!(matches!(rota(&format!("shvia://chat?q={no_limite}")), Rota::Destino(_)));
        assert_eq!(rota(&format!("shvia://chat?q={no_limite}a")), Rota::Recusada);
    }

    #[test]
    fn a_settings_section_is_a_slug_and_nothing_else() {
        assert_eq!(
            rota("shvia://configuracoes/github"),
            Rota::Destino("/chat?configuracoes=github".into())
        );
        assert_eq!(rota("shvia://settings/contas-claude"), Rota::Destino("/chat?configuracoes=contas-claude".into()));
        for ruim in [
            "shvia://configuracoes",
            "shvia://configuracoes/a/b",
            "shvia://configuracoes/GitHub",
            "shvia://configuracoes/..%2Fadmin",
            "shvia://configuracoes/x%3Fq%3D1",
        ] {
            assert_eq!(rota(ruim), Rota::Recusada, "{ruim}");
        }
    }

    /// The link never names the server or the path: every destination it can produce is one the
    /// shell accepts, and none of them points outside `/chat`.
    #[test]
    fn no_link_reaches_another_path_or_origin() {
        for link in [
            "shvia://chat?q=x",
            "shvia://chat/../../admin/users?q=x",
            "shvia://chat@evil.example/?q=x",
            "shvia://configuracoes/github",
        ] {
            if let Rota::Destino(d) = rota(link) {
                assert!(destino_permitido(&d), "{link} → {d}");
                assert!(d.starts_with("/chat"), "{link} → {d}");
            }
        }
        for outro in ["https://ai.shvia.org/chat", "javascript:alert(1)", "shvia-dev://chat", "file:///etc/passwd"] {
            assert_eq!(rota(outro), Rota::Recusada, "{outro}");
        }
        assert_eq!(rota("shvia://admin"), Rota::Recusada);
    }

    #[test]
    fn the_shell_guard_accepts_only_chat() {
        assert!(destino_permitido("/chat"));
        assert!(destino_permitido("/chat?q=oi"));
        for ruim in ["", "/", "/chatx", "//evil.example/chat", "https://evil.example/chat", "/admin?x=/chat"] {
            assert!(!destino_permitido(ruim), "{ruim}");
        }
    }

    /// Declared source check: `src/main.ts` repeats `destino_permitido`. If one side learns a
    /// new path and the other does not, the link opens the dashboard instead of the chat and
    /// nothing says why — so the two are kept written alike here.
    #[test]
    fn the_shell_repeats_the_same_guard() {
        let casca = include_str!("../../src/main.ts");
        assert!(
            casca.contains(r#"ir === "/chat" || ir.startsWith("/chat?")"#),
            "src/main.ts no longer carries the guard `destino_permitido` mirrors"
        );
    }

    #[test]
    fn the_shell_address_carries_the_destination_encoded() {
        assert_eq!(caminho_da_casca("/chat"), "index.html?ir=%2Fchat");
        assert_eq!(caminho_da_casca("/chat?q=a+b%26c"), "index.html?ir=%2Fchat%3Fq%3Da%2Bb%2526c");
    }

    #[test]
    fn a_link_argument_is_recognized_by_its_scheme_only() {
        assert!(e_link("shvia://rapida"));
        assert!(e_link("SHVIA:chat"));
        assert!(!e_link("shviax://rapida"));
        assert!(!e_link("--minimized"));
        assert!(!e_link("sh"));
    }

    #[test]
    fn a_second_link_window_waits_for_the_interval() {
        let t0 = std::time::Instant::now();
        assert!(passou_o_intervalo(None, t0));
        assert!(!passou_o_intervalo(Some(t0), t0 + INTERVALO / 2));
        assert!(passou_o_intervalo(Some(t0), t0 + INTERVALO));
    }

    #[test]
    fn the_log_never_carries_the_prompt() {
        assert_eq!(para_o_log("shvia://chat?q=segredo"), "shvia://chat");
    }
}
