//! ShvIA Desktop — shell fino Tauri 2.
//!
//! A janela abre a casca local (`src/`), que mostra um splash com a marca e
//! redireciona o WebView para o ShvIA hospedado (`https://ia.blue3.com.br`).
//! A partir daí a UI é o próprio Blade do ShvIA — "mesmas funções" (ADR-002).
//!
//! Postura de menor privilégio: **nenhum comando nativo é exposto à página
//! remota** — o servidor é a fonte da verdade; o cliente não abre banco nem
//! guarda segredo. Detalhes em `docs/arquitetura.md` e `docs/decisoes.md`.
//!
//! **Multi-janela (F2):** o menu `Arquivo → Nova janela` (`Ctrl/Cmd+N`) abre mais
//! janelas nativas do ShvIA. Todas compartilham a partição do WebView (cookie de
//! sessão) → mesmo login em cada uma. Útil para ver conversas/projetos lado a lado.

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    Manager, WebviewUrl, WebviewWindowBuilder,
};

/// Abre mais uma janela do ShvIA, com rótulo único `win-N` (não colide com as
/// janelas abertas). Carrega a mesma casca local, que redireciona ao ShvIA; como
/// a partição do WebView é compartilhada, a nova janela já entra logada.
fn open_new_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    let open = app.webview_windows();
    let mut n = open.len() + 1;
    while open.contains_key(&format!("win-{n}")) {
        n += 1;
    }
    WebviewWindowBuilder::new(app, format!("win-{n}"), WebviewUrl::App("index.html".into()))
        .title("ShvIA")
        .inner_size(1280.0, 800.0)
        .min_inner_size(800.0, 600.0)
        .build()?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .menu(|handle| {
            let nova_janela = MenuItem::with_id(
                handle,
                "new-window",
                "Nova janela",
                true,
                Some("CmdOrCtrl+N"),
            )?;
            let arquivo = Submenu::with_items(
                handle,
                "Arquivo",
                true,
                &[
                    &nova_janela,
                    &PredefinedMenuItem::separator(handle)?,
                    &PredefinedMenuItem::close_window(handle, Some("Fechar janela"))?,
                    &PredefinedMenuItem::quit(handle, Some("Sair"))?,
                ],
            )?;
            Menu::with_items(handle, &[&arquivo])
        })
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "new-window" {
                let _ = open_new_window(app);
            }
        })
        .run(tauri::generate_context!())
        .expect("erro ao executar o aplicativo ShvIA Desktop");
}
