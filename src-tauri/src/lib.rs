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

/// Injetado em cada página (`on_page_load`): **ponte de colar imagem (Ctrl+V)**.
/// O WebKitGTK **não entrega a imagem no `clipboardData` do evento `paste`** (só
/// texto/HTML) — embora `navigator.clipboard.read()` **consiga** ler a imagem.
/// Sem isto, colar um print no ShvIA não funciona no desktop Linux (some no
/// Chrome/Firefox, que preenchem o `clipboardData`). A ponte: ao colar, se o
/// evento veio **sem** imagem, lê a imagem do clipboard e **re-despacha um
/// `paste` sintético** com ela num `DataTransfer` — transparente para o handler
/// do app (recebe `clipboardData.items` com a imagem, como num navegador comum).
/// Marca o evento sintético (`__shviaSynthetic`) para não entrar em laço.
const CLIPBOARD_IMAGE_PASTE_JS: &str = r#"(function () {
  if (window.__shviaClipboardBridge) return;
  window.__shviaClipboardBridge = true;
  document.addEventListener('paste', function (e) {
    if (e.__shviaSynthetic) return;
    var dt = e.clipboardData;
    var hasImg = dt && Array.prototype.some.call(dt.items || [], function (it) {
      return it.type && it.type.indexOf('image/') === 0;
    });
    if (hasImg) return;
    if (!navigator.clipboard || !navigator.clipboard.read) return;
    var target = e.target;
    navigator.clipboard.read().then(function (items) {
      for (var i = 0; i < items.length; i++) {
        var it = items[i];
        var t = (it.types || []).find(function (x) { return x.indexOf('image/') === 0; });
        if (!t) continue;
        it.getType(t).then(function (blob) {
          var file = new File([blob], 'pasted.png', { type: blob.type || 'image/png' });
          var d = new DataTransfer();
          d.items.add(file);
          var evt = new ClipboardEvent('paste', { clipboardData: d, bubbles: true, cancelable: true });
          evt.__shviaSynthetic = true;
          (target || document.activeElement || document.body).dispatchEvent(evt);
        });
        return;
      }
    }).catch(function () {});
  }, true);
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
                let _ = webview.eval(CLIPBOARD_IMAGE_PASTE_JS);
            }
        })
        .build()?;
    // habilita mídia (getUserMedia) + clipboard no WebKitGTK e concede a permissão.
    #[cfg(target_os = "linux")]
    configure_linux_webview(&win);
    // restaura geometria salva (no 1º run não há estado: fica no tamanho default).
    let _ = win.restore_state(StateFlags::all());
    let _ = win.show();
    Ok(win)
}

/// Ajusta o WebKitGTK (Linux) para o que o ShvIA precisa e o WebView deixa **off
/// por padrão**:
/// - `enable-media-stream` / `enable-mediasource` → habilita `getUserMedia`
///   (microfone/câmera); sem isso o navegador nem expõe a API e o app reporta
///   "permissão negada";
/// - `javascript-can-access-clipboard` → permite colar/copiar (ex.: Ctrl+V de print);
/// - trata o signal `permission-request` concedendo os pedidos de **mídia**.
///
/// macOS/Windows têm caminhos próprios (Info.plist / WebView2) — tratados ao
/// empacotar lá.
#[cfg(target_os = "linux")]
fn configure_linux_webview(window: &WebviewWindow) {
    use webkit2gtk::{glib::prelude::*, PermissionRequestExt, SettingsExt, WebViewExt};
    let _ = window.with_webview(|wv| {
        let webview = wv.inner();
        if let Some(settings) = WebViewExt::settings(&webview) {
            settings.set_enable_media_stream(true);
            settings.set_enable_mediasource(true);
            settings.set_enable_webrtc(true);
            settings.set_javascript_can_access_clipboard(true);
        }
        webview.connect_permission_request(|_, req| {
            if req
                .downcast_ref::<webkit2gtk::UserMediaPermissionRequest>()
                .is_some()
            {
                req.allow();
                true
            } else {
                false
            }
        });
    });
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
    // WebKitGTK + driver NVIDIA proprietário: o renderer DMABUF deixa o WebView
    // em **branco** — a janela e o menu nativos aparecem, mas o conteúdo web não
    // desenha (visto no `.deb` numa workstation GTX 1060). Desligamos o DMABUF
    // antes de qualquer init de GTK/WebView. Só toca Linux (macOS = WKWebView,
    // Windows = WebView2, onde a env var é inócua e o `cfg` já os exclui);
    // incondicional de propósito, pois o bug não é exclusivo da NVIDIA — já
    // atingiu Mesa/AMD/Intel em versões do WebKitGTK. O custo num WebView de chat
    // é imperceptível. Quem quiser reativar o DMABUF localmente pode exportar
    // `WEBKIT_DISABLE_DMABUF_RENDERER=0` (a env do ambiente tem precedência).
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

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
            // Recarregar: puxa a versão nova do ShvIA hospedado (o app é uma casca
            // fina — recarregar o WebView = pegar o que está no servidor agora).
            let recarregar = MenuItem::with_id(
                handle,
                "reload",
                "Recarregar",
                true,
                Some("CmdOrCtrl+R"),
            )?;
            let arquivo = Submenu::with_items(
                handle,
                "Arquivo",
                true,
                &[
                    &nova_janela,
                    &recarregar,
                    &PredefinedMenuItem::separator(handle)?,
                    &PredefinedMenuItem::close_window(handle, Some("Fechar janela"))?,
                    &PredefinedMenuItem::quit(handle, Some("Sair"))?,
                ],
            )?;
            Menu::with_items(handle, &[&arquivo])
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "new-window" => {
                let _ = open_new_window(app);
            }
            "reload" => {
                // Recarrega a janela em foco; se não achar foco, recarrega todas.
                let windows = app.webview_windows();
                match windows.values().find(|w| w.is_focused().unwrap_or(false)) {
                    Some(win) => {
                        let _ = win.eval("window.location.reload()");
                    }
                    None => {
                        for win in windows.values() {
                            let _ = win.eval("window.location.reload()");
                        }
                    }
                }
            }
            _ => {}
        })
        .setup(|app| {
            build_shvia_window(app.handle(), "main")?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erro ao executar o aplicativo ShvIA Desktop");
}
