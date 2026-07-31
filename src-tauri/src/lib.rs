//! ShvIA Desktop — shell fino Tauri 2.
//!
//! Cada janela abre a casca local (`src/`), que mostra um splash com a marca e
//! redireciona o WebView para o ShvIA hospedado (`https://ai.shvia.org`).
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
//! - rotear **links externos** (fora dos hosts do servidor, `SERVER_HOSTS`) para o **navegador do
//!   SO** via `on_navigation` (login do ShvIA é same-origin, então não quebra auth);
//! - **persistir** tamanho/posição entre reinícios (`tauri-plugin-window-state`).

mod cli_config;
mod code_bridge;
/// Endereço do servidor: config persistida, validação e probe (item D4; ADR-019).
mod server;
#[cfg(desktop)]
mod diagnostico;
#[cfg(desktop)]
mod tray;
/// Auto-update: checa o manifesto que o ShvIA serve, pergunta e instala (D1; ADR-022).
#[cfg(desktop)]
mod updater;
#[cfg(target_os = "macos")]
mod macos_ipc;
#[cfg(target_os = "windows")]
mod windows_ipc;

use tauri::{
    webview::{NewWindowResponse, PageLoadEvent},
    Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
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

/// Injetado nas páginas remotas (`on_page_load`): **gate de compatibilidade
/// cliente↔servidor** (item D6 do comparativo 9router × hermes; ADR-018).
///
/// Lê `version.clients.desktop` do `GET /api/v1/health` (ShvIA 2.64.0) e compara
/// com o build desta casca. Duas situações, dois tratamentos:
///
/// - **Casca abaixo do `min_version`** — tarja de aviso, com o motivo e o link do
///   changelog. Dispensável, e o "dispensar" é lembrado por VERSÃO DO SERVIDOR: se
///   o servidor subir de novo pedindo outra coisa, o aviso volta.
/// - **Casca abaixo do `latest_version`** — nada na tela. "Existe uma versão nova"
///   não é problema; virar tarja para isso é o caminho mais curto para o usuário
///   aprender a ignorar tarjas.
///
/// ## AVISA, não bloqueia — e é decisão, não preguiça
///
/// Esta casca é fina: a UI é o Blade do próprio servidor. Na quase totalidade dos
/// casos o cliente velho **funciona**, só perde uma ponte nativa nova. Bloquear
/// transformaria um `min_version` digitado errado no servidor numa interrupção
/// total. Ver ADR-018 e o `config/clients.php` do ShvIA.
///
/// ## Fail-open em cada passo
///
/// Sem canal nativo, sem rede, JSON inesperado, versão não-parseável: **no-op**. Um
/// gate que se engana e atrapalha é pior que gate nenhum, porque o custo cai em cima
/// de quem está tentando trabalhar.
const VERSION_GATE_JS: &str = r#"(function () {
  if (window.__shviaVersionGate) return;
  window.__shviaVersionGate = true;

  var BUILD = '__SHVIA_BUILD__';
  var KEY = 'shvia_vg_dismissed'; // guarda a versão de servidor já dispensada

  // Compara "0.13.0" com "0.9.2" numericamente, campo a campo. Comparar como
  // string diria que 0.9.2 > 0.13.0, que é o erro clássico aqui.
  function cmp(a, b) {
    var x = String(a || '').split('.'), y = String(b || '').split('.');
    for (var i = 0; i < Math.max(x.length, y.length); i++) {
      var n = parseInt(x[i] || '0', 10), m = parseInt(y[i] || '0', 10);
      if (isNaN(n) || isNaN(m)) return 0;   // não-parseável → empate → no-op
      if (n !== m) return n > m ? 1 : -1;
    }
    return 0;
  }

  function tarja(texto, url) {
    if (document.getElementById('shvia-vg')) return;
    var d = document.createElement('div');
    d.id = 'shvia-vg';
    d.setAttribute('role', 'status');
    d.style.cssText = 'position:fixed;left:0;right:0;bottom:0;z-index:2147483500;' +
      'display:flex;gap:12px;align-items:center;justify-content:center;flex-wrap:wrap;' +
      'padding:10px 16px;font:500 .875rem/1.4 var(--font-body,system-ui,sans-serif);' +
      'color:var(--tx,#ECF1F8);background:var(--bg-elev,#1d2330);' +
      'border-top:1px solid var(--bd,#333b4d);box-shadow:0 -4px 16px rgba(0,0,0,.25)';

    var span = document.createElement('span');
    span.textContent = texto;
    d.appendChild(span);

    if (url) {
      var a = document.createElement('a');
      a.href = url;                      // host externo → o Rust manda pro navegador
      a.textContent = 'Ver o que mudou';
      a.style.cssText = 'color:var(--ac,#7aa2ff);text-decoration:underline';
      d.appendChild(a);
    }

    var b = document.createElement('button');
    b.type = 'button';
    b.textContent = 'Dispensar';
    b.style.cssText = 'padding:4px 10px;border:1px solid var(--bd,#333b4d);border-radius:6px;' +
      'background:transparent;color:inherit;font:inherit;cursor:pointer';
    b.onclick = function () {
      try { localStorage.setItem(KEY, d.getAttribute('data-srv') || '1'); } catch (e) {}
      d.remove();
    };
    d.appendChild(b);
    document.body.appendChild(d);
    return d;
  }

  fetch('/api/v1/health', { headers: { 'Accept': 'application/json' }, credentials: 'same-origin' })
    .then(function (r) { return r.ok ? r.json() : null; })
    .then(function (j) {
      var c = j && j.version && j.version.clients && j.version.clients.desktop;
      if (!c || !c.min_version) return;                       // servidor antigo → no-op
      if (cmp(BUILD, c.min_version) >= 0) return;             // em dia → nada na tela

      // Dispensar é lembrado por versão DO SERVIDOR: se ele subir de novo pedindo
      // outra coisa, o aviso volta. Guardar só um booleano faria o usuário
      // dispensar uma vez e nunca mais ser avisado.
      var srv = String((j.version && j.version.app) || '');
      try { if (localStorage.getItem(KEY) === srv) return; } catch (e) {}

      var texto = c.notice
        ? String(c.notice)
        : 'Este ShvIA Desktop (' + BUILD + ') é mais antigo que o mínimo suportado pelo servidor (' +
          c.min_version + '). Ele deve seguir funcionando, mas vale atualizar.';
      var el = tarja(texto, c.changelog_url || '');
      if (el) { el.setAttribute('data-srv', srv); }
    })
    .catch(function () {});   // rede/JSON ruim → no-op
})();"#;

/// Injetado nas páginas remotas do ShvIA (`on_page_load`): **notificações nativas do
/// SO + contagem no ícone** (ADR-011).
///
/// Faz *polling* de `GET /api/v1/notifications?unread=1` (cookie de sessão
/// same-origin) e posta `{action:'notify'}` e `{action:'badge'}` no **mesmo canal
/// nativo do Modo Code** (`shviaCode` no WebKit, `window.chrome.webview` no
/// WebView2) — o Rust dispara a notificação do SO e a contagem no dock. Assim o
/// evento chega com a janela em segundo plano, sem depender do Telegram.
///
/// ## Por que a rota é GENÉRICA (0.13.0)
///
/// Até a 0.12 o poll era em `/api/v1/price-alerts?unread=1`, que conhecia **um**
/// tipo de evento. O ShvIA 2.60–2.63 passou a produzir notificação para resultado de
/// **rotina**, fim de **lote** e aviso de **destino de entrega morto** — todos pelo
/// mesmo `DeliveryRouter` — e nenhum deles chegava aqui. A rota genérica faz o
/// desktop acompanhar o servidor sem precisar de um poll novo por feature.
///
/// O badge é postado em TODO poll (não só quando há novidade): é o que faz a
/// contagem **zerar** quando o usuário lê no painel, e é o único sinal persistente
/// depois que o toast do SO desaparece — ver `code_bridge::badge` para o porquê de
/// não haver clique→navegar.
///
/// Sem o canal nativo (navegador puro/mobile) é no-op. Dedup persistente por id em
/// `localStorage` (`shvia_pt_notified`) evita repetir e coordena múltiplas janelas.
/// Autocontido, ES5, self-guard. **Não é comando/IPC Tauri** — mantém o ADR-001.
const NATIVE_NOTIFY_JS: &str = r#"(function () {
  if (window.__shviaPriceAlerts) return;
  var wk = window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.shviaCode;
  var w2 = window.chrome && window.chrome.webview;
  if (!wk && !w2) return; // sem canal nativo → sem notificação nativa (badge cobre)
  window.__shviaPriceAlerts = true;

  var STORE_KEY = 'shvia_pt_notified';
  var POLL_MS = 60000;
  var MAX_INDIVIDUAL = 3; // acima disso, uma notificação-resumo

  function sendNative(obj) {
    obj.__t = '__SHVIA_BRIDGE_TOKEN__'; // token de capacidade (ver code_bridge::bridge_token)
    var str = JSON.stringify(obj);
    if (wk) { wk.postMessage(str); } else { w2.postMessage(str); }
  }
  function loadSeen() {
    try { var a = JSON.parse(localStorage.getItem(STORE_KEY) || '[]'); return Array.isArray(a) ? a : []; }
    catch (e) { return []; }
  }
  function saveSeen(a) {
    // Cap alto para não descartar ids ainda no conjunto não-lido (senão eles
    // voltariam a contar como novos e re-notificariam).
    try { localStorage.setItem(STORE_KEY, JSON.stringify(a.slice(-400))); } catch (e) {}
  }
  function poll() {
    // O ShvIA consome /api/v1 com o Bearer token do localStorage (mesmo do app);
    // mandamos ele + o cookie de sessão como fallback. Sem token/sessão → 401 → no-op.
    var token = '';
    try { token = localStorage.getItem('access_token') || ''; } catch (e) {}
    var headers = { 'Accept': 'application/json' };
    if (token) { headers['Authorization'] = 'Bearer ' + token; }
    // Rota GENÉRICA (ShvIA 2.63.0): todo evento que passa pelo DeliveryRouter do
    // servidor chega aqui — alerta de preço, resultado de rotina, fim de lote,
    // aviso de destino de notificação morto. Antes o poll era em
    // `/price-alerts?unread=1`, que conhecia UM tipo de evento e não escalava.
    fetch('/api/v1/notifications?unread=1&limit=50', { headers: headers, credentials: 'same-origin' })
      .then(function (r) { return r.ok ? r.json() : null; })
      .then(function (j) {
        if (!j || !Array.isArray(j.data)) return; // não logado / sem dados → no-op

        // Badge SEMPRE, mesmo sem novidade: é o único sinal persistente depois que
        // o toast do SO desaparece, e o servidor manda a contagem TOTAL de
        // não-lidas (não a da página). Também zera quando o usuário lê no painel.
        sendNative({ action: 'badge', count: Number(j.unread_count) || 0 });

        var seen = loadSeen();
        var fresh = j.data.filter(function (a) { return a && a.id != null && seen.indexOf(a.id) === -1; });
        if (!fresh.length) return;
        // Marca como visto ANTES de notificar. Reduz duplicatas entre janelas
        // (localStorage não tem compare-and-set atômico, então janelas que leem
        // antes de qualquer uma gravar podem notificar 1x cada — aceitável, sem
        // blast); o dedup sequencial nos polls seguintes é garantido.
        fresh.forEach(function (a) { seen.push(a.id); });
        saveSeen(seen);

        if (fresh.length > MAX_INDIVIDUAL) {
          sendNative({ action: 'notify', title: 'ShvIA',
            body: fresh.length + ' novas notificacoes. Abra o ShvIA para ver.' });
          return;
        }
        fresh.forEach(function (a) {
          // `subject`/`body` vêm achatados do servidor de propósito — a casca não
          // precisa conhecer a estrutura interna do Laravel.
          var titulo = a.subject ? ('ShvIA — ' + a.subject) : 'ShvIA';
          var corpo = a.body || a.subject || '';
          if (!corpo) return;
          sendNative({ action: 'notify', title: titulo, body: corpo });
        });
      })
      .catch(function () {});
  }

  setTimeout(poll, 5000);   // deixa a página assentar antes do 1º poll
  setInterval(poll, POLL_MS);
})();"#;

/// Injetado sob demanda (menu `Ajuda → Sobre o ShvIA Desktop`): **modal "Sobre"**
/// com o nome do app, o build do desktop e a versão do ShvIA no servidor — o
/// equivalente ao Help → About do VS Code. Mesmo padrão das outras pontes
/// (`eval`, ES5, autocontido; nada de IPC exposto à página remota — postura de
/// menor privilégio, ver `docs/arquitetura.md`).
///
/// - **Build (desktop)**: o Rust substitui `__SHVIA_BUILD__` pela versão do
///   pacote (`version.md` → tauri.conf.json), `__SHVIA_TAURI__` pela versão do
///   crate `tauri` e `__SHVIA_SERVER_HOST__` por [`SERVER_HOST`], antes do
///   `eval`. Placeholder novo aqui exige `.replace` novo no handler do menu — é
///   uma via de `eval` só, mas se esquecer, a string literal vaza pra UI.
/// - **Servidor**: numa página REMOTA a linha mostra o `location.hostname` que a
///   janela carregou de fato — durante a migração de domínio a pergunta do
///   suporte é "esse binário aponta pra onde?", e casar o host contra uma lista
///   fixa mentiria no dia em que um host novo entrasse. Só na casca local (sem
///   host remoto) cai no canônico compilado.
/// - **ShvIA (servidor)**: o rodapé da sidebar (`.account-mini__version`,
///   dashboard.blade.php) dá o valor imediato — mas ele é do load da página, e
///   a janela pode estar aberta há dias; então `GET /api/v1/health`
///   (`version.app`) é consultado **sempre** e corrige o valor se o servidor
///   foi atualizado. Sem rodapé e com fetch falho (login/offline), "—". O fetch
///   é **relativo** e só sai em página remota: da casca local a origem é
///   `tauri://localhost`, que o CORS do servidor não libera (nem deve), então
///   absoluto seria barrado na leitura e cairia em "—" de qualquer jeito.
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
  // Linha "Servidor" do modal. Numa página REMOTA, mostra o host que a janela
  // realmente carregou — é a pergunta que o suporte faz durante a migração
  // ("esse binário aponta pra onde?"), e uma lista fixa aqui mentiria no dia em
  // que um host novo entrasse. Só na casca local (tauri/localhost, sem host
  // remoto nenhum) cai no canônico compilado.
  var remoto = /^https?:$/.test(location.protocol) && !/^(localhost|tauri\.localhost)$/.test(location.hostname);
  var host = remoto ? location.hostname : '__SHVIA_SERVER_HOST__';

  var overlay = document.createElement('div');
  overlay.id = 'shvia-about-overlay';
  var card = document.createElement('div');
  card.className = 'shvia-about-card';
  card.setAttribute('role', 'dialog');
  card.setAttribute('aria-modal', 'true');
  card.setAttribute('aria-labelledby', 'shvia-about-title');
  card.innerHTML =
    '<h2 class="shvia-about-title" id="shvia-about-title">ShvIA Desktop</h2>' +
    '<p class="shvia-about-sub">Cliente desktop do ShvIA</p>' +
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
  // SEMPRE relativo, e só quando a página é remota. Tentar um FQDN absoluto a
  // partir da casca local (origem tauri://localhost) é beco sem saída: o CORS do
  // servidor só libera as origens de FRONT_DOOR_ORIGINS (config/cors.php), a
  // casca não está — e não deve estar — nessa lista, então o navegador barra a
  // LEITURA da resposta e a linha cai em "—" de qualquer maneira. Relativo
  // funciona em qualquer host do servidor sem lista para manter; na casca local
  // não há versão de servidor para mostrar, e "—" é a resposta honesta.
  if (remoto) {
    fetch('/api/v1/health', { headers: { 'Accept': 'application/json' } })
      .then(function (r) { return r.ok ? r.json() : null; })
      .then(function (j) {
        var v = j && j.version && j.version.app;
        if (v) { setServer('v' + String(v).replace(/^v/, '')); }
        else if (!temRodape) { setServer('—'); }
      })
      .catch(function () { if (!temRodape) { setServer('—'); } });
  } else if (!temRodape) { setServer('—'); }

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

/// Host CANÔNICO do servidor do ShvIA — o destino da navegação e o que o modal
/// "Sobre" informa quando não há host remoto na página.
const SERVER_HOST: &str = "ai.shvia.org";

/// Hosts EXATOS aceitos como o servidor do ShvIA (fonte da verdade). Governa duas
/// coisas: a navegação que fica dentro do app (`is_internal`) e a injeção das
/// pontes nativas (BRIDGE_JS, NATIVE_NOTIFY_JS) no `on_page_load`.
///
/// Lista exata, **sem sufixo curinga**, porque este é o perímetro de segurança do
/// app: um curinga `.shvia.org` ou `.blue3.com.br` deixaria qualquer subdomínio
/// comprometido carregar aqui dentro e ganhar spawn de processo local, leitura de
/// FS e o token de capacidade. Cada entrada abaixo foi verificada por DNS:
///
/// - `ai.shvia.org` — canônico (200.36.196.254).
/// - `ia.shvia.org` — CNAME de `ai.shvia.org`, mesma instância.
/// - `ia.blue3.com.br` — domínio legado, MESMO IP. Fica só durante a transição;
///   remover esta linha é tudo o que o desligamento dele exige.
///
/// O ápex `shvia.org` está FORA de propósito: ele resolve para outro IP
/// (170.233.231.20) e serve a landing, não o app. Precisa abrir no navegador do
/// SO como qualquer link externo.
///
/// No MESMO IP do ápex passou a viver `mem.shvia.org` (servidor ai-memory, TLS
/// na 443, desde 31/07/2026) — e ele é o melhor argumento contra o curinga que
/// esta lista recusa: é `.shvia.org`, é nosso, é legítimo, e mesmo assim **não
/// pode entrar**. O que ele serve é uma wiki markdown **escrita por agentes**;
/// dar `window.__shviaCode` a ela seria transformar texto gerado em spawn de
/// processo local. Um sufixo curinga teria admitido esse host sozinho, no dia
/// em que o DNS subiu, sem ninguém decidir nada.
const SERVER_HOSTS: &[&str] = &[SERVER_HOST, "ia.shvia.org", "ia.blue3.com.br"];

/// `true` se o host for uma das faces do servidor do ShvIA — a lista embutida
/// **ou** o servidor que o dono da máquina configurou (item D4).
///
/// O host configurado entra aqui de propósito, e é a decisão mais pesada do D4:
/// ele passa a receber as **pontes nativas** (Modo Code com spawn de processo,
/// leitura de FS, notificação, badge, token de capacidade). Não existe meio termo
/// útil — casca que abre o servidor do cliente sem as pontes é um navegador, não
/// o ShvIA Desktop.
///
/// O que torna isso aceitável, e o que **precisa continuar valendo**:
/// - a URL só entra por uma tela **nativa** da casca local, digitada por quem
///   está no teclado; página remota nenhuma consegue chamar os comandos que
///   gravam isso (a capability não declara `remote`, então o ACL recusa — ver
///   `capabilities/default.json` e ADR-001);
/// - `https` é obrigatório fora de loopback ([`server::normalize`]);
/// - a tela diz, em português, que o servidor ganha acesso nativo.
pub(crate) fn is_server_host(host: &str) -> bool {
    if SERVER_HOSTS.contains(&host) {
        return true;
    }
    server::configured_host().is_some_and(|h| h == host)
}

/// Configuração do servidor para a casca local desenhar a tela.
#[tauri::command]
fn shvia_server_config(app: tauri::AppHandle) -> server::ServerConfig {
    server::load(&app)
}

/// `true` se alguém aceita conexão no endereço. Substitui o `fetch` com `no-cors`
/// que a casca fazia: o primeiro request do WebKit frio custava 5-6 s (ADR-012) e
/// um `connect-src` estático não pode listar URL digitada pelo usuário.
///
/// `spawn_blocking` porque DNS + connect bloqueiam, e a thread da UI não pode
/// parar — é ela que anima o sonar do splash.
#[tauri::command]
async fn shvia_server_probe(url: String) -> bool {
    tauri::async_runtime::spawn_blocking(move || server::probe(&url))
        .await
        .unwrap_or(false)
}

/// Grava o servidor. Devolve `Err` com texto pronto para a tela.
#[tauri::command]
fn shvia_server_set(app: tauri::AppHandle, url: String) -> Result<server::ServerConfig, String> {
    server::save(&app, &url)
}

/// Volta ao servidor embutido.
#[tauri::command]
fn shvia_server_reset(app: tauri::AppHandle) -> server::ServerConfig {
    server::reset(&app)
}

/// Contador monotônico de janelas abertas por `target=_blank` (handler
/// `on_new_window`). Garante labels únicos.
static OPEN_WINDOW_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Devolve um id novo (>= 1) a cada chamada. Wrap para usar em format!.
fn open_window_counter() -> u64 {
    OPEN_WINDOW_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
}

/// Uma navegação fica **no app** se for a casca local (dev ou empacotada) ou o
/// servidor do ShvIA em https. Qualquer outra origem — incluindo outros
/// subdomínios blue3.com.br — é link externo e abre no navegador do SO.
///
/// A casca EMPACOTADA muda de URL por SO (Tauri `tauri_protocol_url`): no
/// Windows/Android é `http://tauri.localhost`, mas no **macOS e Linux** é
/// `tauri://localhost` — esquema `tauri`, não `http`. Omitir esse esquema aqui
/// bloqueia a navegação inicial do app empacotado e a janela abre BRANCA
/// (0.9.0 no macOS): em dev o `devUrl` é `http://localhost:1420`, então o bug
/// só aparece no build. Ver docs/decisoes.md (ADR-013).
fn is_internal(url: &tauri::Url) -> bool {
    let host = match url.host_str() {
        Some(h) => h,
        None => return false,
    };
    // Casca local: `http` (Vite dev + prod Windows/Android) e `tauri` (prod
    // macOS/Linux). O servidor exige https.
    match url.scheme() {
        "https" => is_server_host(host),
        "http" => host == "localhost" || host == "tauri.localhost",
        "tauri" => host == "localhost",
        _ => false,
    }
}

/// Recria a janela principal, ignorando o erro.
///
/// Existe para a bandeja (item D2) e para o `Reopen` do macOS: os dois querem "traga o
/// app de volta" e nenhum dos dois tem o que fazer com um `Result` — falhar ali só
/// deixaria o app sem janela, que é o estado que se está tentando sair.
#[cfg(desktop)]
pub(crate) fn rebuild_main_window(app: &tauri::AppHandle) {
    if let Err(e) = build_shvia_window(app, "main") {
        eprintln!("ShvIA: não foi possível recriar a janela principal: {e}");
    }
}

/// Cria uma janela do ShvIA com o comportamento padrão do shell: links externos
/// no navegador do SO e estado (tamanho/posição) restaurado e persistido.
fn build_shvia_window(app: &tauri::AppHandle, label: &str) -> tauri::Result<WebviewWindow> {
    let nav_handle = app.clone();
    let win_handle = app.clone();
    let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title("ShvIA")
        .on_navigation(move |url| {
            if is_internal(url) {
                return true;
            }
            // link externo → abre no navegador do SO, não dentro do app.
            let _ = nav_handle.opener().open_url(url.to_string(), None::<&str>);
            false
        })
        // `target=_blank` (e window.open): sem handler, no Windows/WebKit morre
        // silencioso; no Linux vira `target=_blank` morto. Link externo → abre
        // no navegador do SO; link interno → cria nova janela ShvIA.
        .on_new_window(move |url, features| {
            if is_internal(&url) {
                let label = format!("win-open-{}", open_window_counter());
                let title = "ShvIA";
                match WebviewWindowBuilder::new(
                    &win_handle,
                    &label,
                    WebviewUrl::External(url.clone()),
                )
                .title(title)
                .window_features(features)
                .build()
                {
                    Ok(window) => NewWindowResponse::Create { window },
                    Err(_) => NewWindowResponse::Deny,
                }
            } else {
                let _ = win_handle.opener().open_url(url.to_string(), None::<&str>);
                NewWindowResponse::Deny
            }
        })
        // injeta a tarja "Sistema Offline" (+ ponte de clipboard) em cada página.
        // A tarja fica SÓ nas páginas remotas do ShvIA: a casca local (0.5.5)
        // tem UI própria de offline (splash com "Tentar novamente") — com a
        // tarja junto ficava indicador em dobro (visto no macOS, 07/07).
        .on_page_load(|webview, payload| {
            if let PageLoadEvent::Finished = payload.event() {
                let host = payload.url().host_str().unwrap_or_default();
                // As pontes injetam APIs nativas (spawn de processo, leitura de
                // FS, notificações). Restringimos aos hosts EXATOS do servidor do
                // ShvIA (SERVER_HOSTS): a casca local não tem sessão/alertas e
                // qualquer outro host — inclusive o ápex shvia.org, que é a
                // landing em outro IP — é externo.
                if is_server_host(host) {
                    let _ = webview.eval(OFFLINE_BANNER_JS);
                    // Notificações nativas dos alertas de preço — ADR-011. Injeta o
                    // token de capacidade da sessão (só páginas de SERVER_HOSTS o
                    // recebem — são as três faces da MESMA instância; iframe
                    // cross-origin e casca local continuam de fora).
                    let _ = webview.eval(code_bridge::inject_token(NATIVE_NOTIFY_JS));
                    // Gate de versão (ADR-018): compara este build com o
                    // `version.clients.desktop.min_version` do servidor.
                    let _ = webview.eval(VERSION_GATE_JS.replace(
                        "__SHVIA_BUILD__",
                        // A versão do PACOTE (version.md → tauri.conf.json), a mesma
                        // que o modal Sobre mostra. `tauri::VERSION` seria a do
                        // framework, que não é o que o servidor compara.
                        &webview.package_info().version.to_string(),
                    ));
                    let _ = webview.eval(CLIPBOARD_IMAGE_PASTE_JS);
                    // Ponte do Modo Code (window.__shviaCode/__shviaDesktop). Ver code_bridge.rs.
                    let _ = webview.eval(code_bridge::inject_token(code_bridge::BRIDGE_JS));
                }
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

    // Item D2 (ADR-024): fechar a ÚLTIMA janela RECOLHE para a bandeja em vez de
    // encerrar — é o que mantém o app vivo para o alerta de preço chegar com a janela
    // fechada, que era o buraco do ADR-011.
    //
    // `hide()` e não destruir: recolher tem de devolver a janela como ela estava.
    // Destruir e recriar no clique da bandeja recarregaria a página remota — o usuário
    // perderia a rolagem da conversa e pagaria um page load para "voltar" de algo que
    // nunca deveria ter saído.
    //
    // Só a ÚLTIMA. Com duas janelas abertas, `Cmd+W` fecha aquela ali, como em qualquer
    // app multi-janela — recolher a primeira de duas seria um bug com cara de feature.
    #[cfg(desktop)]
    {
        let alvo = win.clone();
        win.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = alvo.app_handle();
                // `<= 1` e não `== 1`: durante o fechamento a contagem pode já não
                // incluir esta janela, dependendo do SO. Errar para o lado de recolher
                // é melhor que errar para o lado de sair com o app devendo um alerta.
                let ultima = app.webview_windows().len() <= 1;
                if ultima && tray::ler(app).close_to_tray {
                    api.prevent_close();
                    let _ = alvo.hide();
                    tray::avisar_uma_vez(app);
                }
            }
        });
    }

    // habilita mídia (getUserMedia) + clipboard no WebKitGTK e concede a permissão.
    #[cfg(target_os = "linux")]
    configure_linux_webview(&win);

    // Ponte do Modo Code no macOS (WKScriptMessageHandler `shviaCode`) — espelha
    // o handler do Linux; ver macos_ipc.rs.
    #[cfg(target_os = "macos")]
    macos_ipc::install(&win);

    // Ponte do Modo Code no Windows (WebView2 WebMessageReceived) — espelha o
    // handler do macOS/Linux; ver windows_ipc.rs.
    #[cfg(target_os = "windows")]
    windows_ipc::install(&win);

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
    let code_window = window.clone();
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

            // ── Ponte do Modo Code (shviaCode) — MESMO canal WebKit da TTS, sem
            // IPC Tauri (ADR-001). A página posta {action, reqId, …}; o Rust
            // spawna/fala com o sidecar `anna` e responde via eval. Ver code_bridge.rs.
            ucm.register_script_message_handler("shviaCode");
            let cw = code_window.clone();
            ucm.connect_script_message_received(Some("shviaCode"), move |_ucm, result| {
                if let Some(v) = result.js_value() {
                    crate::code_bridge::handle_message(&cw, v.to_str().as_ref());
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

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // Notificações nativas dos alertas de preço (ADR-011). Só a API Rust é
        // usada (code_bridge::notify); nenhuma capability exposta à página remota.
        .plugin(tauri_plugin_notification::init())
        .manage(code_bridge::Sidecars::default());

    // window-state (geometria), menu nativo e multi-janela são **desktop-only**
    // (mobile é single-window, sem barra de menu). O `let builder` sombreado só
    // existe no desktop; no mobile o builder segue direto pro `.setup()`.
    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_window_state::Builder::default().build())
        // Auto-update (D1; ADR-022). Registrado sem capability: quem dirige é o
        // `updater.rs` pela API Rust — a página remota não alcança o plugin.
        .plugin(tauri_plugin_updater::Builder::new().build())
        // "Iniciar com o sistema" (item D2; ADR-024). LaunchAgent no macOS, chave Run
        // no Windows, .desktop em ~/.config/autostart no Linux — o plugin cuida das
        // três. Sem capability: quem liga e desliga é o menu da bandeja, em Rust.
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
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
            // Editar: no macOS (WKWebView) os atalhos Cmd+C/V/X/A/Z só funcionam
            // com um menu "Editar" nativo — os itens padrão carregam os selectors
            // do responder chain do Cocoa. Sem isto, não dá nem pra colar num input.
            let editar = Submenu::with_items(
                handle,
                "Editar",
                true,
                &[
                    &PredefinedMenuItem::undo(handle, Some("Desfazer"))?,
                    &PredefinedMenuItem::redo(handle, Some("Refazer"))?,
                    &PredefinedMenuItem::separator(handle)?,
                    &PredefinedMenuItem::cut(handle, Some("Recortar"))?,
                    &PredefinedMenuItem::copy(handle, Some("Copiar"))?,
                    &PredefinedMenuItem::paste(handle, Some("Colar"))?,
                    &PredefinedMenuItem::select_all(handle, Some("Selecionar Tudo"))?,
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
            // Checagem manual do updater (D1; ADR-022). A automática é a cada 6 h e
            // silenciosa; este item existe porque quem acabou de ouvir "tem versão
            // nova" não quer esperar o próximo ciclo — e porque ele passa por cima
            // de um "Depois" clicado antes.
            let atualizar = MenuItem::with_id(
                handle,
                "check-update",
                "Verificar atualizações…",
                true,
                None::<&str>,
            )?;
            // Ajuda → Diagnóstico (item D7; ADR-025). Vizinho do "Sobre" de propósito:
            // o Sobre EXIBE (build, host, WebView) e este VERIFICA — quem procura um
            // procura o outro, e separá-los faria o usuário achar só o que não resolve.
            let diagnostico = MenuItem::with_id(
                handle,
                "diagnostics",
                "Diagnóstico…",
                true,
                None::<&str>,
            )?;
            let ajuda = Submenu::with_items(
                handle,
                "Ajuda",
                true,
                &[
                    &atualizar,
                    &PredefinedMenuItem::separator(handle)?,
                    &diagnostico,
                    &sobre,
                ],
            )?;
            Menu::with_items(handle, &[&arquivo, &editar, &ajuda])
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "new-window" => {
                let _ = open_new_window(app);
            }
            "check-update" => {
                updater::verificar_agora(app);
            }
            "diagnostics" => {
                diagnostico::abrir(app);
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
                    .replace("__SHVIA_TAURI__", tauri::VERSION)
                    .replace("__SHVIA_SERVER_HOST__", SERVER_HOST);
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

    let app = builder
        // Os ÚNICOS comandos do app. A capability `default` não declara `remote`,
        // então o ACL do Tauri recusa `invoke` vindo de página remota — o ADR-001
        // ("nenhum comando exposto à página do servidor") continua valendo, e é o
        // que impede um servidor comprometido de se auto-configurar como destino.
        .invoke_handler(tauri::generate_handler![
            shvia_server_config,
            shvia_server_probe,
            shvia_server_set,
            shvia_server_reset,
        ])
        .setup(|app| {
            // ANTES de abrir a janela: o `is_server_host` é consultado na primeira
            // navegação, e sem isto o servidor configurado seria tratado como link
            // externo e abriria no navegador do SO.
            server::load(app.handle());
            build_shvia_window(app.handle(), "main")?;
            // Depois da janela: o updater espera 20 s antes da primeira checagem,
            // mas agendar antes de haver janela deixaria um diálogo nativo sem
            // janela-mãe se a rede fosse instantânea.
            #[cfg(desktop)]
            updater::agendar(app.handle());
            // Bandeja depois da janela: o menu mostra o servidor configurado, que só
            // existe depois do `server::load` acima. E `instalar` não pode derrubar o
            // `setup` — app sem bandeja é degradação; app que não abre é falha.
            #[cfg(desktop)]
            if let Err(e) = tray::instalar(app.handle()) {
                eprintln!("ShvIA: não foi possível criar o ícone de bandeja: {e}");
                // Sem bandeja, "fechar mantém rodando" (D2) esconderia o app SEM VOLTA:
                // não há janela e não há ícone para reabrir. Desligar a preferência aqui
                // é o que impede o app de virar um processo invisível — descoberto ao
                // escrever o item D7, que precisava reportar exatamente esta combinação.
                tray::desligar_recolher(app.handle());
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("erro ao construir o aplicativo ShvIA Desktop");

    // Ciclo de vida do sidecar (anti-órfão): mata o `anna` da janela ao fechá-la
    // e todos ao sair do app. (O `anna` também sai sozinho quando o stdin fecha.)
    app.run(|app_handle, event| match event {
        tauri::RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::Destroyed,
            ..
        } => {
            app_handle.state::<code_bridge::Sidecars>().kill_one(&label);
        }
        // macOS: clicar no ícone do Dock com todas as janelas fechadas não
        // recria nada por padrão. Recria a principal para o usuário.
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen { .. } => {
            if app_handle.webview_windows().is_empty() {
                let _ = build_shvia_window(app_handle, "main");
            }
        }
        tauri::RunEvent::Exit => {
            app_handle.state::<code_bridge::Sidecars>().kill_all();
        }
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use super::is_internal;

    fn internal(url: &str) -> bool {
        is_internal(&url.parse().expect("url de teste válida"))
    }

    /// A casca empacotada tem URL diferente por SO. Se qualquer uma destas
    /// deixar de ser interna, a navegação inicial é bloqueada e o app abre com a
    /// janela BRANCA naquele SO — sem sintoma em `tauri dev` (que usa devUrl).
    #[test]
    fn casca_local_e_interna_nos_tres_sos() {
        assert!(internal("tauri://localhost/index.html")); // prod macOS/Linux
        assert!(internal("http://tauri.localhost/index.html")); // prod Windows/Android
        assert!(internal("http://localhost:1420/")); // dev (Vite)
    }

    #[test]
    fn servidor_so_em_https() {
        assert!(internal("https://ai.shvia.org/chat"));
        assert!(!internal("http://ai.shvia.org/chat"));
    }

    /// Dual-host da migração (26/07): as três faces do servidor entram, e o dia
    /// em que o legado sair é só tirar a linha dele de SERVER_HOSTS.
    #[test]
    fn as_tres_faces_do_servidor_sao_internas() {
        assert!(internal("https://ai.shvia.org/chat"));
        assert!(internal("https://ia.shvia.org/chat"));
        assert!(internal("https://ia.blue3.com.br/chat"));
    }

    /// O ápex shvia.org é a LANDING, em outro IP (170.233.231.20) — não o app.
    /// Se ele entrasse aqui, um site que não é o ShvIA ganharia a ponte nativa
    /// (spawn de processo local, leitura de FS, token de capacidade).
    ///
    /// `mem.shvia.org` (ai-memory, no mesmo IP do ápex) está na lista porque é o
    /// caso REAL, não hipotético: subdomínio nosso, legítimo, no ar — servindo
    /// wiki escrita por agentes. É exatamente o que um curinga `.shvia.org`
    /// deixaria entrar sem ninguém decidir nada.
    #[test]
    fn apex_shvia_org_e_externo() {
        assert!(!internal("https://shvia.org/"));
        assert!(!internal("https://www.shvia.org/"));
        assert!(!internal("https://mem.shvia.org/"));
        assert!(!internal("https://evil.shvia.org/"));
        assert!(!internal("https://ai.shvia.org.evil.com/"));
    }

    /// Item D4: o servidor configurado pelo dono da máquina vira **interno** — é o
    /// que faz o on-prem receber Modo Code, notificação e badge. Continua sendo
    /// UM host: configurar `meu.servidor` não abre a vizinhança dele.
    ///
    /// Estes testes mexem no estado global de `server::CONFIGURED_HOST`, então
    /// ficam num único `#[test]` — dois testes concorrentes na mesma thread pool
    /// se atropelariam.
    #[test]
    fn servidor_configurado_vira_interno_e_so_ele() {
        super::server::set_configured_host_para_teste("https://onprem.cliente.example:8443");

        assert!(internal("https://onprem.cliente.example:8443/chat"));
        // Mesmo host em outra porta: `is_server_host` compara HOST, e a porta não
        // faz parte da identidade de origem para este perímetro.
        assert!(internal("https://onprem.cliente.example/chat"));

        // Vizinhança do host configurado NÃO entra.
        assert!(!internal("https://outro.cliente.example/"));
        assert!(!internal("https://onprem.cliente.example.evil.com/"));
        // E http continua fora, mesmo sendo o host configurado.
        assert!(!internal("http://onprem.cliente.example/"));

        // Os embutidos seguem valendo junto com o configurado.
        assert!(internal("https://ai.shvia.org/chat"));

        // Limpa: outros testes deste módulo assumem só a lista embutida.
        super::server::set_configured_host_para_teste(super::server::DEFAULT_URL);
        assert!(!internal("https://onprem.cliente.example/chat"));
    }

    /// O endurecimento do 0.9.0: nada além do host canônico entra no app (nem
    /// outro subdomínio blue3.com.br), pra ponte nativa não vazar.
    #[test]
    fn outras_origens_sao_externas() {
        assert!(!internal("https://blue3.com.br/"));
        assert!(!internal("https://evil.blue3.com.br/"));
        assert!(!internal("https://ia.blue3.com.br.evil.com/"));
        assert!(!internal("tauri://evil/"));
        assert!(!internal("file:///etc/passwd"));
    }
}
