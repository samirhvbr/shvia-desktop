//! The quick window: a global shortcut shows a small ShvIA chat over any app (1.10.0, ADR-038).
//!
//! ## What it is
//!
//! A second kind of window, labelled [`JANELA`], that loads the same chat page as the others —
//! the shell has no chat of its own (ADR-002), so the quick window is the ShvIA page in a small
//! frame that floats above the other apps. The shortcut toggles it; closing it hides it. It is
//! created once and kept, so the conversation in it is still there the next time.
//!
//! ## What was taken from OpenClaw, and what was not
//!
//! OpenClaw's Quick Chat (`apps/macos/Sources/OpenClaw/QuickChatController.swift`) is a native
//! panel. From it: the window appears on the screen the pointer is on, centered, its top at 22 %
//! of the visible area (`QuickChatPlacement.swift`); the shortcut toggles; it hides, it is not
//! destroyed. Not taken, because our window holds a web page and theirs is native:
//!
//! - **It does not hide when it loses focus.** The page opens the native file picker to attach a
//!   file, and that takes the focus; the window would vanish mid-attach.
//! - **Escape is the page's.** It closes the page's own dialogs; hiding on it would take the
//!   window away from someone closing a menu.
//!
//! ## The shortcut
//!
//! Chosen from a short list in the tray menu, never typed: a recorder is a screen of its own,
//! and the list covers the chords that are free on the three systems. The default,
//! `Ctrl+Shift+Espaço` (`⌘⇧Espaço` on macOS), is bound by none of them out of the box;
//! `Alt+Espaço` is Windows' window menu and GNOME's, so it is offered, not chosen.
//!
//! A chord another program already holds fails to register. That is reported where the choice
//! is made — the tray says so next to it — and the app starts anyway: a missing shortcut is a
//! degradation, not a reason to stay closed.
//!
//! On **Wayland** there is no global shortcut for an app to take (the protocol does not have
//! one; the plugin grabs through XWayland, which only sees keys while an X11 app has focus).
//! The way there is the system's own shortcut settings, bound to `xdg-open shvia://rapida`
//! (`esquema.rs`), and the tray says that instead of pretending.

use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// The quick window's label. Every place that counts or picks "the app's windows" leaves it out
/// ([`normais`]): it is hidden most of the time, and a hidden window chosen as "the main one" is
/// a click on the tray that shows nothing.
pub const JANELA: &str = "rapida";

const PREFS_FILE: &str = "janela-rapida.json";

/// Size of the quick window, in logical pixels. Narrow enough to sit over the work, tall enough
/// for SHVIA-WEB's compact layout to show a conversation and its composer.
pub const LARGURA: f64 = 520.0;
pub const ALTURA: f64 = 680.0;

/// The chords the tray offers. `id` is what is stored, so a label can change without losing
/// anyone's choice; `acelerador` is the plugin's syntax.
pub struct Opcao {
    pub id: &'static str,
    pub acelerador: &'static str,
    pub rotulo: &'static str,
}

pub const OPCOES: &[Opcao] = &[
    Opcao {
        id: "ctrl-shift-espaco",
        acelerador: "CommandOrControl+Shift+Space",
        rotulo: if cfg!(target_os = "macos") { "⌘⇧Espaço" } else { "Ctrl+Shift+Espaço" },
    },
    Opcao {
        id: "ctrl-alt-espaco",
        acelerador: "CommandOrControl+Alt+Space",
        rotulo: if cfg!(target_os = "macos") { "⌘⌥Espaço" } else { "Ctrl+Alt+Espaço" },
    },
    Opcao {
        id: "alt-espaco",
        acelerador: "Alt+Space",
        rotulo: if cfg!(target_os = "macos") { "⌥Espaço" } else { "Alt+Espaço" },
    },
];

/// Stored when the person turned the shortcut off.
pub const DESLIGADO: &str = "desligado";

/// The stored choice, read the way `tray::Prefs` is: a missing, unreadable or unknown value is
/// the default, never "off". Turning the shortcut off is only ever the person's own click.
pub fn escolha_do_json(v: &serde_json::Value) -> &'static str {
    match v.get("atalho").and_then(|x| x.as_str()) {
        Some(DESLIGADO) => DESLIGADO,
        Some(id) => OPCOES.iter().find(|o| o.id == id).map_or(OPCOES[0].id, |o| o.id),
        None => OPCOES[0].id,
    }
}

fn prefs_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join(PREFS_FILE))
}

pub fn escolha(app: &AppHandle) -> &'static str {
    let v = prefs_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .unwrap_or(serde_json::Value::Null);
    escolha_do_json(&v)
}

fn gravar_escolha(app: &AppHandle, id: &str) {
    let Some(path) = prefs_path(app) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::json!({ "atalho": id }).to_string());
}

/// Why the shortcut is not working right now, for the tray to show. `None` = registered, or
/// turned off on purpose.
static FALHA: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub fn falha() -> Option<String> {
    FALHA.lock().ok().and_then(|g| g.clone())
}

/// Registers the stored chord (and only it). Called at startup and after every choice in the
/// tray. Never fails the caller: the outcome goes to [`falha`].
pub fn aplicar_atalho(app: &AppHandle) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let id = escolha(app);
    let resultado = match OPCOES.iter().find(|o| o.id == id) {
        None => Ok(()),
        Some(o) => gs.register(o.acelerador).map_err(|e| e.to_string()),
    };
    if let Ok(mut g) = FALHA.lock() {
        *g = resultado.err();
    }
}

/// The tray's choice. Returns the error when the chord could not be registered, so the tray can
/// say it right where the person clicked.
pub fn escolher(app: &AppHandle, id: &str) -> Option<String> {
    gravar_escolha(app, id);
    aplicar_atalho(app);
    falha()
}

/// The plugin's handler. Only one chord is ever registered, so any press is the toggle.
/// `Pressed` only: the release of the same keys would toggle it straight back.
pub fn ao_atalho(app: &AppHandle, _atalho: &Shortcut, estado: ShortcutState) {
    if estado == ShortcutState::Pressed {
        alternar(app);
    }
}

/// Is this session one where no app can take a global shortcut?
pub fn sessao_wayland() -> bool {
    cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some()
}

/// What the shortcut (or `shvia://rapida`) does, from the window's state. Pure, so the four
/// cases are tested.
#[derive(Debug, PartialEq, Eq)]
pub enum Acao {
    /// No quick window yet: create it, place it and show it.
    Criar,
    /// Hidden or minimized: place it on the pointer's screen and show it.
    Mostrar,
    /// On screen behind another app: bring it forward. Hiding here would make the first press
    /// after clicking elsewhere look like it did nothing.
    Focar,
    /// On screen and in front: the person pressed it to put it away.
    Esconder,
}

pub fn decidir(existe: bool, visivel: bool, minimizada: bool, focada: bool) -> Acao {
    match (existe, visivel && !minimizada, focada) {
        (false, _, _) => Acao::Criar,
        (true, false, _) => Acao::Mostrar,
        (true, true, false) => Acao::Focar,
        (true, true, true) => Acao::Esconder,
    }
}

/// Where the window goes on a screen, in physical pixels: centered, its top at 22 % of the
/// screen's height (OpenClaw's `QuickChatPlacement.swift`), pulled up if it would pass the
/// bottom, never above the top.
pub fn posicao(tela_pos: (i32, i32), tela_tam: (u32, u32), janela: (u32, u32)) -> (i32, i32) {
    let (tx, ty) = tela_pos;
    let (tw, th) = (tela_tam.0 as i64, tela_tam.1 as i64);
    let (jw, jh) = (janela.0 as i64, janela.1 as i64);
    let x = tx as i64 + (tw - jw).max(0) / 2;
    let topo = th * 22 / 100;
    let y = ty as i64 + topo.min((th - jh).max(0));
    (x as i32, y as i32)
}

/// Toggles the quick window. The shortcut, `shvia://rapida` and the tray all come here.
pub fn alternar(app: &AppHandle) {
    let janela = app.get_webview_window(JANELA);
    let acao = match &janela {
        None => decidir(false, false, false, false),
        Some(w) => decidir(
            true,
            w.is_visible().unwrap_or(false),
            w.is_minimized().unwrap_or(false),
            w.is_focused().unwrap_or(false),
        ),
    };
    match (acao, janela) {
        (Acao::Criar, _) => match criar(app) {
            Ok(w) => mostrar(app, &w),
            Err(e) => eprintln!("ShvIA: não foi possível abrir a janela rápida: {e}"),
        },
        (Acao::Mostrar, Some(w)) => mostrar(app, &w),
        (Acao::Focar, Some(w)) => {
            let _ = w.set_focus();
        }
        (Acao::Esconder, Some(w)) => {
            let _ = w.hide();
        }
        _ => {}
    }
}

fn criar(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let destino = crate::esquema::caminho_da_casca("/chat");
    crate::build_shvia_window(app, JANELA, WebviewUrl::App(destino.into()))
}

/// Places it on the screen the pointer is on, then shows and focuses it. The screen is read at
/// every show, not at creation: with two monitors, the window comes to where the person is.
fn mostrar(app: &AppHandle, w: &WebviewWindow) {
    let tela = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| w.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    if let (Some(m), Ok(tam)) = (tela, w.outer_size()) {
        let area = m.work_area();
        let (x, y) = posicao(
            (area.position.x, area.position.y),
            (area.size.width, area.size.height),
            (tam.width, tam.height),
        );
        let _ = w.set_position(PhysicalPosition::new(x, y));
    }
    let _ = w.unminimize();
    let _ = w.show();
    let _ = w.set_focus();
}

/// The app's own windows: every ShvIA window except the quick one.
pub fn normais(app: &AppHandle) -> Vec<WebviewWindow> {
    app.webview_windows()
        .into_iter()
        .filter(|(label, _)| label != JANELA)
        .map(|(_, w)| w)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_states_of_the_toggle() {
        assert_eq!(decidir(false, false, false, false), Acao::Criar);
        assert_eq!(decidir(true, false, false, false), Acao::Mostrar);
        assert_eq!(decidir(true, true, true, true), Acao::Mostrar, "minimized counts as hidden");
        assert_eq!(decidir(true, true, false, false), Acao::Focar);
        assert_eq!(decidir(true, true, false, true), Acao::Esconder);
    }

    #[test]
    fn it_sits_centered_with_its_top_at_22_percent() {
        assert_eq!(posicao((0, 0), (1920, 1080), (520, 680)), (700, 237));
        assert_eq!(posicao((0, 0), (1366, 768), (520, 680)), (423, 88), "pulled up, bottom on screen");
    }

    #[test]
    fn a_second_monitor_is_offset_by_its_own_origin() {
        assert_eq!(posicao((1920, 0), (2560, 1440), (1040, 1360)), (1920 + 760, 80));
        assert_eq!(posicao((-1280, -200), (1280, 1024), (520, 680)), (-1280 + 380, -200 + 225));
    }

    #[test]
    fn a_window_larger_than_the_screen_starts_at_its_corner() {
        assert_eq!(posicao((0, 0), (400, 300), (520, 680)), (0, 0));
    }

    #[test]
    fn the_stored_choice_falls_back_to_the_default_never_to_off() {
        let d = OPCOES[0].id;
        assert_eq!(escolha_do_json(&serde_json::Value::Null), d);
        assert_eq!(escolha_do_json(&serde_json::json!({})), d);
        assert_eq!(escolha_do_json(&serde_json::json!({"atalho": "f13"})), d, "an id this version does not know");
        assert_eq!(escolha_do_json(&serde_json::json!({"atalho": 3})), d);
        assert_eq!(escolha_do_json(&serde_json::json!({"atalho": "desligado"})), DESLIGADO);
        assert_eq!(escolha_do_json(&serde_json::json!({"atalho": "alt-espaco"})), "alt-espaco");
    }

    /// Every chord in the list parses: a typo here would be a tray option that silently
    /// registers nothing.
    #[test]
    fn every_offered_chord_parses() {
        for o in OPCOES {
            assert!(o.acelerador.parse::<Shortcut>().is_ok(), "{}", o.acelerador);
        }
        let ids: std::collections::HashSet<_> = OPCOES.iter().map(|o| o.id).collect();
        assert_eq!(ids.len(), OPCOES.len(), "two options share an id");
        assert!(!ids.contains(&DESLIGADO));
    }
}
