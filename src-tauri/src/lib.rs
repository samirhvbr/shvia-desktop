//! ShvIA Desktop — shell fino Tauri 2.
//!
//! Cada janela abre a casca local (`src/`), que mostra um splash com a marca e
//! redireciona o WebView para o ShvIA hospedado (`https://ia.blue3.com.br`).
//! A partir daí a UI é o próprio Blade do ShvIA — "mesmas funções" (ADR-002).
//!
//! Postura de menor privilégio: **nenhum comando nativo é exposto à página
//! remota** — o servidor é a fonte da verdade; o cliente não abre banco nem
//! guarda segredo. Detalhes em `docs/arquitetura.md` e `docs/decisoes.md`.
//!
//! As janelas são criadas **no Rust** (`build_shvia_window`), não no
//! `tauri.conf.json`, para podermos:
//! - **multi-janela** (F2): menu `Arquivo → Nova janela` (`Ctrl/Cmd+N`) — todas
//!   compartilham a sessão (cookie), úteis para conversas/projetos lado a lado;
//! - rotear **links externos** (fora de `*.blue3.com.br`) para o **navegador do
//!   SO** via `on_navigation` (login do ShvIA é same-origin, então não quebra auth);
//! - **persistir** tamanho/posição entre reinícios (`tauri-plugin-window-state`).

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    webview::PageLoadEvent,
    Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_window_state::{StateFlags, WindowExt};

/// Injetado em cada página carregada (`on_page_load`): uma **tarja vermelha
/// "Sistema Offline"** que aparece quando o WebView perde conexão (eventos
/// `online`/`offline` do navegador) e some ao reconectar; clicável para recarregar.
/// O `eval` do Tauri roda fora da CSP da página, então a injeção funciona mesmo
/// na página remota do ShvIA. (v1 — baseada em `navigator.onLine`.)
const OFFLINE_BANNER_JS: &str = r#"(function () {
  if (window.__shviaOffline) return;
  window.__shviaOffline = true;
  var bar = document.createElement('div');
  bar.id = 'shvia-offline-bar';
  bar.textContent = '⚠  Sistema Offline — clique para recarregar';
  var s = bar.style;
  s.position='fixed'; s.top='0'; s.left='0'; s.right='0'; s.zIndex='2147483647';
  s.background='#c0392b'; s.color='#fff'; s.textAlign='center'; s.padding='8px 12px';
  s.font='600 14px system-ui,sans-serif'; s.letterSpacing='.02em'; s.cursor='pointer';
  s.boxShadow='0 2px 8px rgba(0,0,0,.35)';
  bar.addEventListener('click', function () { location.reload(); });
  function update(){ bar.style.display = navigator.onLine ? 'none' : 'block'; }
  function mount(){ var r=document.body||document.documentElement; if(r&&!document.getElementById('shvia-offline-bar')) r.appendChild(bar); }
  mount(); update();
  window.addEventListener('online', update);
  window.addEventListener('offline', update);
})();"#;

/// Uma navegação fica **no app** se for a casca local (localhost/tauri) ou o
/// ShvIA hospedado (`*.blue3.com.br`); qualquer outra origem é considerada um
/// link externo e abre no navegador do SO.
fn is_internal(url: &tauri::Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    host == "localhost"
        || host == "tauri.localhost"
        || host == "blue3.com.br"
        || host.ends_with(".blue3.com.br")
}

/// Cria uma janela do ShvIA com o comportamento padrão do shell: links externos
/// no navegador do SO e estado (tamanho/posição) restaurado e persistido.
fn build_shvia_window(app: &tauri::AppHandle, label: &str) -> tauri::Result<WebviewWindow> {
    let handle = app.clone();
    let win = WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title("ShvIA")
        // ícone da janela (em dev o GNOME usa isto; no pacote vem do .desktop).
        .icon(tauri::include_image!("icons/icon.png"))?
        .inner_size(1280.0, 800.0)
        .min_inner_size(800.0, 600.0)
        // começa oculta: restauramos o estado antes de mostrar (evita o "pulo").
        .visible(false)
        .on_navigation(move |url| {
            if is_internal(url) {
                return true;
            }
            // link externo → abre no navegador do SO, não dentro do app.
            let _ = handle.opener().open_url(url.to_string(), None::<&str>);
            false
        })
        // injeta a tarja "Sistema Offline" em cada página carregada.
        .on_page_load(|webview, payload| {
            if let PageLoadEvent::Finished = payload.event() {
                let _ = webview.eval(OFFLINE_BANNER_JS);
            }
        })
        .build()?;
    // restaura geometria salva (no 1º run não há estado: fica no tamanho default).
    let _ = win.restore_state(StateFlags::all());
    let _ = win.show();
    Ok(win)
}

/// Abre mais uma janela do ShvIA, com rótulo único `win-N` (não colide com as
/// janelas abertas). Compartilha a partição do WebView, então já entra logada.
fn open_new_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    let open = app.webview_windows();
    let mut n = open.len() + 1;
    while open.contains_key(&format!("win-{n}")) {
        n += 1;
    }
    build_shvia_window(app, &format!("win-{n}"))?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
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
        .setup(|app| {
            build_shvia_window(app.handle(), "main")?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erro ao executar o aplicativo ShvIA Desktop");
}
