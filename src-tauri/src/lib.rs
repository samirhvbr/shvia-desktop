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
    webview::PageLoadEvent, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use tauri_plugin_opener::OpenerExt;
// Menu nativo é desktop-only (mobile não tem barra de menu). O window-state
// (geometria de janela) idem — importado localmente no bloco #[cfg(desktop)] de
// build_shvia_window; a dependência também é desktop-only no Cargo.toml.
#[cfg(desktop)]
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};

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

/// Injetado sob demanda (menu `Ajuda → Sobre o ShvIA Desktop`): **modal "Sobre"**
/// com o nome do app, o build do desktop e a versão do ShvIA no servidor — o
/// equivalente ao Help → About do VS Code. Mesmo padrão das outras pontes
/// (`eval`, ES5, autocontido; nada de IPC exposto à página remota — postura de
/// menor privilégio, ver `docs/arquitetura.md`).
///
/// - **Build (desktop)**: o Rust substitui `__SHVIA_BUILD__` pela versão do
///   pacote (`version.md` → tauri.conf.json) e `__SHVIA_TAURI__` pela versão do
///   crate `tauri` antes do `eval`.
/// - **ShvIA (servidor)**: o rodapé da sidebar (`.account-mini__version`,
///   dashboard.blade.php) dá o valor imediato — mas ele é do load da página, e
///   a janela pode estar aberta há dias; então `GET /api/v1/health`
///   (`version.app`) é consultado **sempre** e corrige o valor se o servidor
///   foi atualizado. Sem rodapé e com fetch falho (login/offline), "—".
/// - **Visual**: usa os design tokens do ShvIA (`tokens.css` do servidor) via
///   `var(--token, fallback)` — na página remota herda o tema real; na casca
///   local os fallbacks reproduzem os mesmos valores. Esc/backdrop fecham,
///   foco vai ao botão e volta ao elemento anterior, `prefers-reduced-motion`
///   desliga as transições. Fica **abaixo** da tarja offline no z-index.
const ABOUT_MODAL_JS: &str = r#"(function () {
  var old = document.getElementById('shvia-about-overlay');
  // Reabrir pelo menu = recriar com dados frescos. O foco a restaurar no fechar
  // vem da instância anterior (se houver): o activeElement de agora seria o
  // botão dela — um nó prestes a ser destacado.
  var prevFocus = (old && old.__shviaPrevFocus) || document.activeElement;
  if (old) { old.remove(); }

  var css = [
    '#shvia-about-overlay{position:fixed;top:0;right:0;bottom:0;left:0;z-index:2147483600;display:flex;align-items:center;justify-content:center;',
    'background:rgba(3,8,14,.62);opacity:0;transition:opacity .16s ease-out}',
    '#shvia-about-overlay.shvia-about-on{opacity:1}',
    '.shvia-about-card{width:min(400px,calc(100vw - 40px));box-sizing:border-box;padding:28px;',
    'background:var(--bg-3,#131A26);border:1px solid var(--line-2,rgba(150,170,200,.18));',
    'border-radius:var(--radius-md,16px);box-shadow:var(--shadow-lg,0 14px 40px rgba(3,8,14,.4));',
    'color:var(--tx,#ECF1F8);font-family:var(--font-sans,system-ui,sans-serif);',
    'transform:scale(.97);opacity:0;transition:transform .2s cubic-bezier(.22,1,.36,1),opacity .2s ease-out}',
    '#shvia-about-overlay.shvia-about-on .shvia-about-card{transform:scale(1);opacity:1}',
    '.shvia-about-title{margin:0;font:600 1.25rem/1.3 var(--font-display,system-ui,sans-serif);letter-spacing:-.01em;color:var(--tx,#ECF1F8)}',
    '.shvia-about-sub{margin:4px 0 20px;font-size:.8rem;color:var(--tx-dim,#97A3B5)}',
    '.shvia-about-rows{display:grid;grid-template-columns:auto 1fr;gap:10px 18px;margin:0 0 16px;align-items:baseline}',
    '.shvia-about-rows dt{margin:0;font:500 .68rem/1.4 var(--font-mono,Consolas,monospace);letter-spacing:.08em;text-transform:uppercase;color:var(--tx-dim,#97A3B5)}',
    '.shvia-about-rows dd{margin:0;font:500 .85rem/1.4 var(--font-mono,Consolas,monospace);font-variant-numeric:tabular-nums;text-align:right;overflow-wrap:anywhere;color:var(--tx,#ECF1F8)}',
    '.shvia-about-env{margin:0 0 22px;font:400 .7rem/1.5 var(--font-mono,Consolas,monospace);color:var(--tx-dim,#97A3B5)}',
    '.shvia-about-actions{display:flex;justify-content:flex-end;gap:10px}',
    '.shvia-about-actions button{font:500 .8rem/1 var(--font-sans,system-ui,sans-serif);padding:9px 16px;border-radius:10px;cursor:pointer;transition:background .15s ease-out,color .15s ease-out}',
    '.shvia-about-actions button:focus-visible{outline:2px solid var(--accent,#34B3EC);outline-offset:2px}',
    '#shvia-about-copy{background:transparent;border:1px solid var(--line-2,rgba(150,170,200,.18));color:var(--tx-dim,#97A3B5)}',
    '#shvia-about-copy:hover{background:var(--bg-item-hover,rgba(255,255,255,.05));color:var(--tx,#ECF1F8)}',
    '#shvia-about-close{background:var(--azure-soft,rgba(52,179,236,.12));border:1px solid rgba(52,179,236,.35);color:var(--azure-strong,#5FC8F5)}',
    '#shvia-about-close:hover{background:rgba(52,179,236,.22)}',
    '@media (prefers-reduced-motion:reduce){#shvia-about-overlay,.shvia-about-card,.shvia-about-actions button{transition:none}}'
  ].join('');
  var style = document.getElementById('shvia-about-style');
  if (!style) {
    style = document.createElement('style');
    style.id = 'shvia-about-style';
    (document.head || document.documentElement).appendChild(style);
  }
  style.textContent = css;

  // Ambiente (SO + WebView + Tauri): ajuda diagnóstico remoto via "Copiar".
  var ua = navigator.userAgent;
  var os = /Windows/i.test(ua) ? 'Windows'
    : /Mac OS X|Macintosh/i.test(ua) ? 'macOS'
    : /Linux/i.test(ua) ? 'Linux' : (navigator.platform || '?');
  var m, wv;
  if (os === 'Windows') {
    m = ua.match(/Chrome\/(\d+)/);
    wv = 'WebView2' + (m ? ' (Chromium ' + m[1] + ')' : '');
  } else {
    m = ua.match(/AppleWebKit\/([\d.]+)/);
    wv = (os === 'macOS' ? 'WKWebView' : 'WebKitGTK') + (m ? ' (WebKit ' + m[1] + ')' : '');
  }
  var env = 'Tauri __SHVIA_TAURI__ · ' + wv + ' · ' + os;
  var host = /(^|\.)blue3\.com\.br$/.test(location.hostname) ? location.hostname : 'ia.blue3.com.br';

  var overlay = document.createElement('div');
  overlay.id = 'shvia-about-overlay';
  var card = document.createElement('div');
  card.className = 'shvia-about-card';
  card.setAttribute('role', 'dialog');
  card.setAttribute('aria-modal', 'true');
  card.setAttribute('aria-labelledby', 'shvia-about-title');
  card.innerHTML =
    '<h2 class="shvia-about-title" id="shvia-about-title">ShvIA Desktop</h2>' +
    '<p class="shvia-about-sub">Cliente desktop do ShvIA · Blue3</p>' +
    '<dl class="shvia-about-rows">' +
    '<dt>Build desktop</dt><dd id="shvia-about-build"></dd>' +
    '<dt>ShvIA servidor</dt><dd id="shvia-about-server" aria-live="polite">…</dd>' +
    '<dt>Servidor</dt><dd id="shvia-about-host"></dd>' +
    '</dl>' +
    '<p class="shvia-about-env" id="shvia-about-env"></p>' +
    '<div class="shvia-about-actions">' +
    '<button type="button" id="shvia-about-copy" aria-live="polite">Copiar</button>' +
    '<button type="button" id="shvia-about-close">Fechar</button>' +
    '</div>';
  overlay.appendChild(card);
  overlay.__shviaPrevFocus = prevFocus;
  (document.body || document.documentElement).appendChild(overlay);

  // Valores dinâmicos sempre via textContent (nunca innerHTML): a versão do
  // servidor vem do DOM/JSON remoto e não deve virar markup. Build com o mesmo
  // prefixo "v" do rodapé do ShvIA, p/ as duas linhas lerem igual.
  card.querySelector('#shvia-about-build').textContent = 'v__SHVIA_BUILD__';
  card.querySelector('#shvia-about-host').textContent = host;
  card.querySelector('#shvia-about-env').textContent = env;

  // Escopado ao card (não getElementById): o fetch de uma instância anterior
  // ainda em voo escreve no próprio nó destacado, nunca no modal novo.
  var serverDd = card.querySelector('#shvia-about-server');
  var serverVersion = '…';
  function setServer(v) {
    serverVersion = v;
    serverDd.textContent = v;
  }
  // Rodapé da sidebar = valor imediato (foi renderizado no load da página);
  // /api/v1/health = fonte viva, consultada SEMPRE (o servidor pode ter sido
  // atualizado com a janela aberta). Se o fetch falhar, fica o rodapé; sem
  // nenhum dos dois (login/offline), "—".
  var vEl = document.querySelector('.account-mini__version');
  var vTxt = vEl && vEl.textContent ? vEl.textContent.trim() : '';
  var temRodape = /^v?\d+(\.\d+)*$/.test(vTxt);
  if (temRodape) { setServer(vTxt); }
  var base = /(^|\.)blue3\.com\.br$/.test(location.hostname) ? '' : 'https://ia.blue3.com.br';
  fetch(base + '/api/v1/health', { headers: { 'Accept': 'application/json' } })
    .then(function (r) { return r.ok ? r.json() : null; })
    .then(function (j) {
      var v = j && j.version && j.version.app;
      if (v) { setServer('v' + String(v).replace(/^v/, '')); }
      else if (!temRodape) { setServer('—'); }
    })
    .catch(function () { if (!temRodape) { setServer('—'); } });

  var copyBtn = card.querySelector('#shvia-about-copy');
  var closeBtn = card.querySelector('#shvia-about-close');

  function close() {
    document.removeEventListener('keydown', onKey, true);
    overlay.classList.remove('shvia-about-on');
    var reduced = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (reduced) { overlay.remove(); }
    else { setTimeout(function () { overlay.remove(); }, 200); }
    if (prevFocus && prevFocus.focus && prevFocus.isConnected) { try { prevFocus.focus(); } catch (e) {} }
  }
  function onKey(e) {
    // Instância órfã (modal recriado por cima): remove o próprio listener.
    if (!overlay.isConnected) { document.removeEventListener('keydown', onKey, true); return; }
    if (e.key === 'Escape') { e.stopPropagation(); close(); return; }
    if (e.key === 'Tab') {  // foco circula entre os dois botões do diálogo
      e.preventDefault();
      var f = [copyBtn, closeBtn];
      var i = f.indexOf(document.activeElement);
      f[e.shiftKey ? (i <= 0 ? f.length - 1 : i - 1) : (i < 0 || i === f.length - 1 ? 0 : i + 1)].focus();
    }
  }
  overlay.addEventListener('mousedown', function (e) { if (e.target === overlay) { close(); } });
  closeBtn.addEventListener('click', close);
  copyBtn.addEventListener('click', function () {
    var text = 'ShvIA Desktop\nBuild (desktop): v__SHVIA_BUILD__\nShvIA (servidor): ' + serverVersion +
      '\nServidor: ' + host + '\n' + env;
    function done(ok) {
      copyBtn.textContent = ok ? 'Copiado ✓' : 'Falhou';
      setTimeout(function () { copyBtn.textContent = 'Copiar'; }, 1600);
    }
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(text).then(function () { done(true); }, function () { done(false); });
    } else { done(false); }
  });
  document.addEventListener('keydown', onKey, true);

  void overlay.offsetWidth;  // aplica o estado inicial antes de transicionar
  overlay.classList.add('shvia-about-on');
  closeBtn.focus();
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
    let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title("ShvIA")
        .on_navigation(move |url| {
            if is_internal(url) {
                return true;
            }
            // link externo → abre no navegador do SO, não dentro do app.
            let _ = handle.opener().open_url(url.to_string(), None::<&str>);
            false
        })
        // injeta a tarja "Sistema Offline" (+ ponte de clipboard) em cada página.
        // A tarja fica SÓ nas páginas remotas do ShvIA: a casca local (0.5.5)
        // tem UI própria de offline (splash com "Tentar novamente") — com a
        // tarja junto ficava indicador em dobro (visto no macOS, 07/07).
        .on_page_load(|webview, payload| {
            if let PageLoadEvent::Finished = payload.event() {
                let host = payload.url().host_str().unwrap_or_default().to_owned();
                let casca_local = host == "localhost" || host == "tauri.localhost";
                if !casca_local {
                    let _ = webview.eval(OFFLINE_BANNER_JS);
                }
                let _ = webview.eval(CLIPBOARD_IMAGE_PASTE_JS);
            }
        });

    // Geometria, ícone e visibilidade da janela são conceitos de **desktop**; no
    // mobile a WebView ocupa a tela toda e não há ícone de janela. No desktop
    // começa oculta p/ restaurar o estado antes de mostrar (evita o "pulo").
    #[cfg(desktop)]
    let builder = builder
        .icon(tauri::include_image!("icons/icon.png"))?
        .inner_size(1280.0, 800.0)
        .min_inner_size(800.0, 600.0)
        .visible(false);

    let win = builder.build()?;

    // habilita mídia (getUserMedia) + clipboard no WebKitGTK e concede a permissão.
    #[cfg(target_os = "linux")]
    configure_linux_webview(&win);

    // window-state (restaurar geometria + mostrar sem "pulo") é desktop-only; no
    // mobile a janela já nasce visível em tela cheia.
    #[cfg(desktop)]
    {
        use tauri_plugin_window_state::{StateFlags, WindowExt};
        let _ = win.restore_state(StateFlags::all());
        let _ = win.show();
    }

    Ok(win)
}

/// Ajusta o WebKitGTK (Linux) para o que o ShvIA precisa e o WebView deixa **off
/// por padrão**:
/// - `enable-media-stream` / `enable-mediasource` → habilita `getUserMedia`
///   (microfone/câmera); sem isso o navegador nem expõe a API e o app reporta
///   "permissão negada";
/// - `javascript-can-access-clipboard` → permite colar/copiar (ex.: Ctrl+V de print);
/// - trata o signal `permission-request` concedendo os pedidos de **mídia**;
/// - registra a **ponte de leitura em voz (TTS)** `shviaTts` (ver ADR-009): o
///   WebKitGTK só traz vozes en-US (Flite) e não enxerga o pt-BR do SO, então a
///   página posta o texto e o Rust fala pelo `spd-say`/espeak-ng.
///
/// macOS/Windows têm caminhos próprios (Info.plist / WebView2) — tratados ao
/// empacotar lá.
#[cfg(target_os = "linux")]
fn configure_linux_webview(window: &WebviewWindow) {
    use javascriptcore::ValueExt;
    use webkit2gtk::{
        glib::prelude::*, PermissionRequestExt, SettingsExt, UserContentManagerExt, WebViewExt,
    };

    // Clone da janela p/ o handler nativo devolver o "terminou" à página (eval).
    let tts_window = window.clone();
    let _ = window.with_webview(move |wv| {
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

        // ── Ponte de leitura em voz (TTS) — ver ADR-009 ──────────────────────
        // O WebKitGTK (Flite) só expõe vozes en-US; o pt-BR do SO (espeak-ng via
        // speech-dispatcher) não chega ao WebView. Então o botão "ouvir" da
        // resposta fala pelo host: a página posta em
        // `window.webkit.messageHandlers.shviaTts` e o Rust roda o `spd-say`.
        // **Não é comando Tauri** — não abre a superfície de IPC à página remota
        // (ADR-001); é o canal nativo do próprio WebKit, fora da CSP da página.
        // Payload: `{ "action": "speak"|"stop", "gen": <n>, "text": "<...>" }`.
        if let Some(ucm) = webview.user_content_manager() {
            ucm.register_script_message_handler("shviaTts");
            let win = tts_window.clone();
            ucm.connect_script_message_received(Some("shviaTts"), move |_ucm, result| {
                let payload = match result.js_value() {
                    Some(v) => v.to_str().to_string(),
                    None => return,
                };
                let parsed: serde_json::Value = match serde_json::from_str(&payload) {
                    Ok(v) => v,
                    Err(_) => return,
                };
                match parsed.get("action").and_then(|a| a.as_str()) {
                    Some("speak") => {
                        let generation =
                            parsed.get("gen").and_then(|g| g.as_u64()).unwrap_or(0);
                        let text = parsed.get("text").and_then(|t| t.as_str()).unwrap_or("");
                        tts_speak(&win, generation, text);
                    }
                    Some("stop") => tts_cancel(),
                    _ => {}
                }
            });
        }
    });
}

/// Fala um texto pela ponte nativa (Linux): `spd-say` → speech-dispatcher →
/// espeak-ng, em **pt-BR**. Roda numa thread (o `-w` bloqueia até a fala terminar
/// **ou** ser descartada) e, ao fim, avisa a página via `window.__shviaTtsEnded(gen)`
/// para o botão voltar de "Parar" a "Ouvir". O `gen` deixa o front descartar o
/// aviso de uma fala antiga quando o usuário troca de resposta rapidamente.
#[cfg(target_os = "linux")]
fn tts_speak(window: &WebviewWindow, generation: u64, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    // Teto defensivo: evita estourar o argv e falas intermináveis.
    let text: String = text.chars().take(16_000).collect();
    let window = window.clone();
    std::thread::spawn(move || {
        use std::process::Command;
        // Troca imediata: cancela qualquer fala em curso antes de começar a nova.
        let _ = Command::new("spd-say").arg("-C").status();
        // Fala e ESPERA (`-w`) terminar ou ser descartada.
        let spoke = Command::new("spd-say")
            .args(["-w", "-o", "espeak-ng", "-l", "pt-BR", "--"])
            .arg(&text)
            .status();
        let ended = format!("window.__shviaTtsEnded&&window.__shviaTtsEnded({generation});");
        // spd-say ausente/quebrado: reseta o botão e avisa (não fica travado e mudo).
        let script = match spoke {
            Ok(_) => ended,
            Err(_) => format!(
                "{ended}window.showToast&&window.showToast('Leitura em voz indisponível neste sistema.','error');"
            ),
        };
        let app = window.app_handle().clone();
        let _ = app.run_on_main_thread(move || {
            let _ = window.eval(&script);
        });
    });
}

/// Para qualquer leitura em voz em curso (Linux). `spd-say -C` cancela no
/// speech-dispatcher; a thread da fala (`-w`) então retorna sozinha e dispara o
/// `__shviaTtsEnded`. (Cancela globalmente no daemon — aceitável neste app.)
#[cfg(target_os = "linux")]
fn tts_cancel() {
    std::thread::spawn(|| {
        let _ = std::process::Command::new("spd-say").arg("-C").status();
    });
}

/// Abre mais uma janela do ShvIA, com rótulo único `win-N` (não colide com as
/// janelas abertas). Compartilha a partição do WebView, então já entra logada.
/// **Multi-janela é desktop-only** (mobile é single-window).
#[cfg(desktop)]
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

    let builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());

    // window-state (geometria), menu nativo e multi-janela são **desktop-only**
    // (mobile é single-window, sem barra de menu). O `let builder` sombreado só
    // existe no desktop; no mobile o builder segue direto pro `.setup()`.
    #[cfg(desktop)]
    let builder = builder
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
            // Ajuda → Sobre (padrão Help → About): identifica o build do desktop
            // separado da versão do ShvIA no servidor (v… do rodapé da sidebar).
            let sobre = MenuItem::with_id(
                handle,
                "about",
                "Sobre o ShvIA Desktop",
                true,
                None::<&str>,
            )?;
            let ajuda = Submenu::with_items(handle, "Ajuda", true, &[&sobre])?;
            Menu::with_items(handle, &[&arquivo, &ajuda])
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
            "about" => {
                // Abre o modal "Sobre" na janela em foco (fallback: a primeira).
                // Build vem do pacote (version.md → tauri.conf.json, via sync).
                let js = ABOUT_MODAL_JS
                    .replace("__SHVIA_BUILD__", &app.package_info().version.to_string())
                    .replace("__SHVIA_TAURI__", tauri::VERSION);
                let windows = app.webview_windows();
                let alvo = windows
                    .values()
                    .find(|w| w.is_focused().unwrap_or(false))
                    .or_else(|| windows.values().next());
                if let Some(win) = alvo {
                    // Sem janela em foco (ex.: macOS com tudo minimizado e menu
                    // global clicável), traz a janela alvo à frente — senão o
                    // modal abre invisível e o menu parece morto.
                    let _ = win.unminimize();
                    let _ = win.set_focus();
                    let _ = win.eval(&js);
                }
            }
            _ => {}
        });

    builder
        .setup(|app| {
            build_shvia_window(app.handle(), "main")?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erro ao executar o aplicativo ShvIA Desktop");
}
