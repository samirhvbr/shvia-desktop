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
mod user_env;
mod code_bridge;
mod contas_claude;
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
/// "Sistema Offline"** clicável para recarregar.
///
/// v2 (01/08/2026, bug real no Linux): a v1 confiava no `navigator.onLine`, e no
/// WebKitGTK ele vem do GNetworkMonitor, que **mente** com VPN/rotas incomuns
/// (Tailscale etc.) — a tarja ficava acesa com o chat funcionando por baixo.
/// Agora `onLine=false` é só um GATILHO DE SUSPEITA: a tarja apenas aparece se
/// uma sonda REAL ao `/up` (health barato do Laravel, ~30 ms) falhar; enquanto
/// visível, re-sonda a cada 15 s e some sozinha quando o servidor voltar —
/// a v1 também dependia do evento `online`, que no WebKitGTK pode nunca vir.
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
  function mount(){ var r=document.body||document.documentElement; if(r&&!document.getElementById('shvia-offline-bar')) r.appendChild(bar); }
  var checking=false, timer=null;
  function show(){ bar.style.display='block'; if(!timer) timer=setInterval(check, 15000); }
  function hide(){ bar.style.display='none'; if(timer){ clearInterval(timer); timer=null; } }
  function check(){
    if (checking) return; checking = true;
    var ctl = ('AbortController' in window) ? new AbortController() : null;
    var to = setTimeout(function(){ if (ctl) ctl.abort(); }, 5000);
    fetch('/up', { cache:'no-store', signal: ctl && ctl.signal })
      .then(function(r){ if (r.ok) { hide(); } else { show(); } })
      .catch(function(){ show(); })
      .finally(function(){ clearTimeout(to); checking = false; });
  }
  function update(){ if (navigator.onLine) { hide(); } else { check(); } }
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
    '<dt>Motor do Code</dt><dd id="shvia-about-anna"></dd>' +
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
  // Motor do Code (`anna`): versão E origem. O Rust já resolveu — a página não
  // tem como perguntar isso, e é justamente o dado que faltava em 19/08.
  card.querySelector('#shvia-about-anna').textContent = '__SHVIA_ANNA__';
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
    // O "Copiar" existe para colar num relato de suporte — então ele carrega as
    // MESMAS linhas do modal. Motor do Code incluído: foi a linha ausente que fez
    // o diagnóstico de 19/08 custar uma manhã.
    var text = 'ShvIA Desktop\nBuild (desktop): v__SHVIA_BUILD__\nShvIA (servidor): ' + serverVersion +
      '\nMotor do Code: __SHVIA_ANNA__\nServidor: ' + host + '\n' + env;
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
/// O que o X faz — decisão isolada de propósito, para ser provável sem janela, sem
/// bandeja e sem sidecar.
///
/// A ordem das cláusulas É a decisão, e cada uma tem um motivo diferente:
///
/// 1. **Recolher ganha de tudo.** Se a janela vai para a bandeja, nada se perde — nem
///    sessão, nem rolagem, nem processo. Perguntar aqui seria pedir confirmação para uma
///    ação sem consequência, que é o jeito mais rápido de ensinar alguém a clicar sem ler.
/// 2. **Perguntar só quando há o que perder.** Com `anna` no ar, fechar encerra a sessão
///    **e** deixa processo órfão — medido em 20/08/2026: dois `anna` vivos e um
///    `shvia-desktop` zumbi desde 06/08 nesta máquina. Aí a pergunta paga por si.
/// 3. **Fechar calado é o certo no resto.** Janela sem Code, ou uma de várias: fechar é o
///    que qualquer app faz, e confirmar viraria atrito sem defesa.
#[cfg(desktop)]
#[derive(Debug, PartialEq, Eq)]
enum AcaoDeFechar {
    Recolher,
    Perguntar,
    Fechar,
}

#[cfg(desktop)]
fn decidir_fechar(ultima: bool, close_to_tray: bool, code_em_voo: bool) -> AcaoDeFechar {
    if ultima && close_to_tray {
        return AcaoDeFechar::Recolher;
    }
    if code_em_voo {
        return AcaoDeFechar::Perguntar;
    }
    AcaoDeFechar::Fechar
}


#[cfg(desktop)]
pub(crate) fn rebuild_main_window(app: &tauri::AppHandle) {
    if let Err(e) = build_shvia_window(app, "main", WebviewUrl::App("index.html".into())) {
        eprintln!("ShvIA: não foi possível recriar a janela principal: {e}");
    }
}

/// Cria uma janela do ShvIA com o comportamento padrão do shell: links externos
/// no navegador do SO e estado (tamanho/posição) restaurado e persistido.
/// A ÚNICA forma de criar janela do ShvIA — inclusive as de `target=_blank`.
///
/// ## Por que a URL virou parâmetro (achado F-15 da revisão de 01/09/2026)
///
/// As janelas de `target=_blank` nasciam de um `WebviewWindowBuilder` próprio, com `title` e
/// `window_features` e **mais nada**: sem `on_navigation`, sem `on_page_load`, sem ícone, sem
/// `min_inner_size`, sem o handler de fechamento.
///
/// A consequência que importa é a primeira: sem `on_navigation`, um link externo clicado
/// dentro dessa janela **navega dentro do app** em vez de ir para o navegador do SO. Um site
/// de terceiro passa a ocupar uma janela intitulada "ShvIA", com a moldura do aplicativo —
/// que é o formato clássico de phishing. Não havia exposição nativa (as pontes também não
/// eram instaladas), então o perímetro rompido era de UX, não de sistema de arquivos.
///
/// Duas janelas com regras diferentes é a mesma forma de defeito que esta revisão encontrou
/// no `containerDo` (E-5) e nos dois caminhos de atualização (G-21): a segunda cópia não
/// nasce errada, ela envelhece sozinha. Agora existe **uma** função, e quem abre janela passa
/// a URL.
fn build_shvia_window(
    app: &tauri::AppHandle,
    label: &str,
    url: WebviewUrl,
) -> tauri::Result<WebviewWindow> {
    let nav_handle = app.clone();
    let win_handle = app.clone();
    let builder = WebviewWindowBuilder::new(app, label, url)
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
        .on_new_window(move |url, _features| {
            if is_internal(&url) {
                let label = format!("win-open-{}", open_window_counter());
                // Reusa a MESMA construção da janela principal (F-15): a janela de
                // `target=_blank` passa a ter `on_navigation` — e portanto link externo
                // dentro dela volta a sair para o navegador do SO —, `on_page_load`, ícone,
                // tamanho mínimo e o handler de fechamento.
                //
                // ⚠️ `window_features` fica de fora de propósito: ela vem da PÁGINA
                // (`window.open(..., "width=300")`), e uma janela de 300px sem barra é o
                // outro formato clássico de phishing. O tamanho da janela do app é decisão
                // do app.
                match build_shvia_window(&win_handle, &label, WebviewUrl::External(url.clone())) {
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
                    // Ponte de voz (window.__shviaTts). Linux-only: é lá que o
                    // handler nativo `shviaTts` existe. Ver TTS_BRIDGE_JS / F-16.
                    #[cfg(target_os = "linux")]
                    let _ = webview.eval(code_bridge::inject_token(TTS_BRIDGE_JS));
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
                let code_em_voo = app
                    .state::<code_bridge::Sidecars>()
                    .tem_sessao(alvo.label());
                match decidir_fechar(ultima, tray::ler(app).close_to_tray, code_em_voo) {
                    AcaoDeFechar::Recolher => {
                        api.prevent_close();
                        let _ = alvo.hide();
                        tray::avisar_uma_vez(app);
                    }
                    AcaoDeFechar::Perguntar => {
                        // Impede AGORA e pergunta depois, em outra thread. `blocking_show`
                        // na main thread trava o app — é o próprio loop de eventos que
                        // precisa girar para o diálogo responder (a mesma regra que o
                        // `updater::perguntar` já documenta).
                        api.prevent_close();
                        let janela = alvo.clone();
                        std::thread::spawn(move || {
                            use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
                            let app = janela.app_handle().clone();
                            let sim = app
                                .dialog()
                                .message(
                                    "O Modo Code está com uma sessão aberta. Fechar encerra a \
                                     sessão e o processo do agente.",
                                )
                                .title("Fechar o ShvIA?")
                                .buttons(MessageDialogButtons::OkCancelCustom(
                                    "Fechar e encerrar".into(),
                                    "Continuar aberto".into(),
                                ))
                                .blocking_show();
                            if sim {
                                // Mata o sidecar ANTES de destruir a janela: depois de
                                // `destroy` o label sai do mapa de janelas e ninguém mais
                                // sabe qual `anna` era desta — é assim que nasce órfão, e
                                // já há um zumbi de 06/08 nesta máquina para provar.
                                app.state::<code_bridge::Sidecars>().kill_one(janela.label());
                                // `destroy`, NUNCA `close`: `close` reemite
                                // `CloseRequested` e cairíamos aqui de novo, perguntando
                                // para sempre.
                                let _ = janela.destroy();
                            }
                        });
                    }
                    AcaoDeFechar::Fechar => {}
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
/// - trata o signal `permission-request` concedendo **só microfone**, e só quando o
///   frame principal é o servidor do ShvIA (ver `permite_midia`);
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
        glib::prelude::*, PermissionRequestExt, SettingsExt, UserContentManagerExt,
        UserMediaPermissionRequestExt, WebViewExt,
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
        webview.connect_permission_request(|wv, req| {
            let Some(midia) = req.downcast_ref::<webkit2gtk::UserMediaPermissionRequest>() else {
                return false;
            };
            let uri = WebViewExt::uri(wv).unwrap_or_default();
            if permite_midia(
                &uri,
                midia.is_for_audio_device(),
                midia.is_for_video_device(),
            ) {
                req.allow();
            } else {
                req.deny();
            }
            true
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
                match pedido_de_voz(&payload, crate::code_bridge::bridge_token()) {
                    Some(PedidoDeVoz::Falar { geracao, texto }) => {
                        tts_speak(&win, geracao, &texto)
                    }
                    Some(PedidoDeVoz::Parar) => tts_cancel(),
                    None => {}
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

/// Shim injected into ShvIA server pages (Linux only) so the page can reach the
/// native voice bridge **with the session's capability token** — finding F-16.
///
/// Mirrors `code_bridge::BRIDGE_JS`: self-contained, ES5, self-guarded (without the
/// native handler it defines nothing, and the web falls back to `speechSynthesis` or
/// the server-side TTS, which is what macOS/Windows already do). The page must go
/// through `window.__shviaTts` — posting to `messageHandlers.shviaTts` by hand no
/// longer works, which is precisely what shuts the embedded iframe out.
#[cfg(target_os = "linux")]
const TTS_BRIDGE_JS: &str = r#"(function () {
  if (window.__shviaTts) return;
  var mh = window.webkit && window.webkit.messageHandlers;
  var wk = mh && mh.shviaTts;
  if (!wk) return;
  function post(msg) {
    msg.__t = '__SHVIA_BRIDGE_TOKEN__'; // capability token (see bridge_token)
    try { wk.postMessage(JSON.stringify(msg)); } catch (e) { /* noop */ }
  }
  window.__shviaTts = {
    speak: function (gen, text) { post({ action: 'speak', gen: gen, text: text }); },
    stop: function () { post({ action: 'stop' }); }
  };
})();"#;

/// Whether a media permission request may be granted without asking (finding F-16).
///
/// Two narrowings over the previous unconditional `allow()`:
/// - the page in the **main frame** must be a ShvIA server host over https — the
///   local shell (`tauri://localhost`) has no capture UI and never needs a device;
/// - only **audio** is granted. SHVIA-WEB never calls `getUserMedia({video})` (its
///   three call sites in `public/js/app.js` all ask for `{audio}`), so granting the
///   camera was a permission with no product behind it.
///
/// **Known limitation, measured 02/09/2026:** WebKitGTK carries no origin and no
/// frame on the request — `webkit2gtk 2.0.2` exposes exactly two getters on
/// `UserMediaPermissionRequest`, `is-for-audio-device` and `is-for-video-device`.
/// So a cross-origin `<iframe>` inside a ShvIA page still asks with the main
/// frame's URI and gets the microphone. To re-check after a crate bump, look for an
/// origin/frame getter in
/// `~/.cargo/registry/src/*/webkit2gtk-*/src/auto/user_media_permission_request.rs`;
/// while there is none, this is as narrow as the API allows.
#[cfg(target_os = "linux")]
fn permite_midia(uri_do_frame_principal: &str, para_audio: bool, para_video: bool) -> bool {
    if para_video || !para_audio {
        return false;
    }
    match tauri::Url::parse(uri_do_frame_principal) {
        Ok(u) => u.scheme() == "https" && u.host_str().is_some_and(is_server_host),
        Err(_) => false,
    }
}

/// What a `shviaTts` payload asks for, once the capability token checks out.
#[cfg(target_os = "linux")]
#[derive(Debug, PartialEq)]
enum PedidoDeVoz {
    Falar { geracao: u64, texto: String },
    Parar,
}

/// Reads a `shviaTts` payload, **dropping anything that does not carry the session's
/// capability token** (finding F-16).
///
/// The native message handler exists for *every* frame of the webview, so before this
/// gate a cross-origin `<iframe>` embedded in a ShvIA page could speak arbitrary text
/// through the host's `spd-say` and cancel speech — and `spd-say -C` cancels globally
/// in the speech-dispatcher daemon, so it reached beyond the app. The token is injected
/// only into pages served by a `SERVER_HOSTS` host (`TTS_BRIDGE_JS` via `on_page_load`),
/// which an iframe cannot read: same gate `code_bridge::handle_message` already applies
/// to `shviaCode`, now covering the second native handler as well.
///
/// Dropping is silent on purpose — a reply would turn the handler into an oracle for
/// guessing the token.
#[cfg(target_os = "linux")]
fn pedido_de_voz(payload: &str, token: &str) -> Option<PedidoDeVoz> {
    let v: serde_json::Value = serde_json::from_str(payload).ok()?;
    if v.get("__t").and_then(|t| t.as_str()) != Some(token) {
        return None;
    }
    match v.get("action").and_then(|a| a.as_str())? {
        "speak" => Some(PedidoDeVoz::Falar {
            geracao: v.get("gen").and_then(|g| g.as_u64()).unwrap_or(0),
            texto: v
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or_default()
                .to_string(),
        }),
        "stop" => Some(PedidoDeVoz::Parar),
        _ => None,
    }
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
    build_shvia_window(app, &format!("win-{n}"), WebviewUrl::App("index.html".into()))?;
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

    // `single_instance` tem de ser o PRIMEIRO plugin registrado — é requisito dele, não
    // preferência: ele decide se este processo continua vivo antes de qualquer outra
    // inicialização, e um plugin que já tenha subido estado precisaria desfazê-lo.
    //
    // Medido em 20/08/2026: dois `shvia-desktop` vivos ao mesmo tempo (19/08 14:08 e
    // 20/08 08:16) mais um zumbi de 06/08. Sem guarda, cada invocação é um app novo — com
    // bandeja própria e contagem de janelas própria —, e aí o `<= 1` do `CloseRequested`
    // decide certo sobre a instância ERRADA: cada uma acha que é a única do mundo.
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
        // Segunda invocação: trazer de volta o que já existe. `show` antes de
        // `unminimize` porque a janela pode estar recolhida na bandeja (hidden), e
        // `unminimize` numa janela oculta não a torna visível.
        if let Some(w) = app.webview_windows().values().next() {
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    }));
    let builder = builder
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
                    .replace("__SHVIA_ANNA__", &code_bridge::versao_do_anna())
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
            build_shvia_window(app.handle(), "main", WebviewUrl::App("index.html".into()))?;
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
                let _ = build_shvia_window(app_handle, "main", WebviewUrl::App("index.html".into()));
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
    #[cfg(desktop)]
    use super::{decidir_fechar, AcaoDeFechar};

    /// Recolher **ganha** de perguntar, e isto é a asserção que importa: com a bandeja
    /// ligada nada se perde, então perguntar seria confirmar uma ação sem consequência —
    /// o caminho mais curto para treinar alguém a clicar sem ler.
    #[test]
    #[cfg(desktop)]
    fn com_bandeja_ligada_recolhe_mesmo_com_code_em_voo() {
        assert_eq!(decidir_fechar(true, true, true), AcaoDeFechar::Recolher);
        assert_eq!(decidir_fechar(true, true, false), AcaoDeFechar::Recolher);
    }

    /// O caso que motivou o item: bandeja desligada (é o que o `desligar_recolher` faz
    /// quando a criação da bandeja falha, e fica PERSISTIDO) + sessão do Code no ar. Sem
    /// este ramo, o X encerra `anna` sem avisar.
    #[test]
    #[cfg(desktop)]
    fn sem_bandeja_e_com_code_em_voo_pergunta() {
        assert_eq!(decidir_fechar(true, false, true), AcaoDeFechar::Perguntar);
    }

    /// Uma de VÁRIAS janelas fecha aquela ali, como em qualquer app multi-janela — mas se
    /// ela tem Code no ar, ainda pergunta: o que se perde é a sessão daquela janela, e
    /// isso não fica menos verdade por haver outra janela aberta.
    #[test]
    #[cfg(desktop)]
    fn janela_do_meio_pergunta_se_tem_code_e_fecha_se_nao_tem() {
        assert_eq!(decidir_fechar(false, true, true), AcaoDeFechar::Perguntar);
        assert_eq!(decidir_fechar(false, true, false), AcaoDeFechar::Fechar);
    }

    /// Sem nada a perder, fecha calado. Confirmação aqui seria atrito sem defesa.
    #[test]
    #[cfg(desktop)]
    fn sem_bandeja_e_sem_code_fecha_calado() {
        assert_eq!(decidir_fechar(true, false, false), AcaoDeFechar::Fechar);
        assert_eq!(decidir_fechar(false, false, false), AcaoDeFechar::Fechar);
    }

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

    /// F-16: o microfone deixa de ser concedido a qualquer página da webview.
    ///
    /// O que muda de fato: antes, TODO `UserMediaPermissionRequest` recebia `allow()`.
    /// Agora a câmera nunca é concedida (o produto nunca a pede — `getUserMedia({audio})`
    /// nos três pontos do `app.js`) e o áudio só quando o frame principal é o servidor.
    #[test]
    #[cfg(target_os = "linux")]
    fn midia_so_com_microfone_e_so_no_servidor() {
        use super::permite_midia;
        // (uri, áudio, vídeo, esperado)
        let casos = [
            ("https://ai.shvia.org/chat", true, false, true),
            ("https://ia.blue3.com.br/", true, false, true),
            // Câmera: negada mesmo no servidor — não há produto atrás dela.
            ("https://ai.shvia.org/chat", false, true, false),
            ("https://ai.shvia.org/chat", true, true, false),
            // Casca local: não tem captura, então não precisa de dispositivo.
            ("tauri://localhost/index.html", true, false, false),
            ("http://localhost:1420/", true, false, false),
            // Terceiros — inclusive o vizinho de sufixo, que o `is_server_host` recusa.
            ("https://evil.com/", true, false, false),
            ("https://ai.shvia.org.evil.com/", true, false, false),
            // http no host do servidor não vale: o servidor é https.
            ("http://ai.shvia.org/", true, false, false),
            ("", true, false, false),
        ];
        for (uri, audio, video, esperado) in casos {
            assert_eq!(
                permite_midia(uri, audio, video),
                esperado,
                "permite_midia({uri:?}, audio={audio}, video={video})"
            );
        }
    }

    /// F-16: a ponte de voz passa a exigir o token de capacidade da sessão.
    ///
    /// O `<iframe>` cross-origin alcança o `messageHandler` nativo (a webview o expõe a
    /// TODO frame) mas não lê o token — que só é injetado nas páginas de `SERVER_HOSTS`.
    /// Sem esta porta ele falava pelo `spd-say` do host e cancelava fala globalmente.
    #[test]
    #[cfg(target_os = "linux")]
    fn a_voz_exige_o_token_da_sessao() {
        use super::{pedido_de_voz, PedidoDeVoz};
        let t = "abc123";
        // Sem token, com token errado, ou com o campo de outro tipo: nada acontece.
        assert_eq!(pedido_de_voz(r#"{"action":"stop"}"#, t), None);
        assert_eq!(pedido_de_voz(r#"{"action":"stop","__t":"outro"}"#, t), None);
        assert_eq!(pedido_de_voz(r#"{"action":"stop","__t":123}"#, t), None);
        assert_eq!(pedido_de_voz(r#"{"action":"speak","text":"oi"}"#, t), None);
        assert_eq!(pedido_de_voz("nao e json", t), None);
        // Com o token certo, o pedido passa — inclusive o `gen` ausente (default 0).
        assert_eq!(
            pedido_de_voz(r#"{"action":"stop","__t":"abc123"}"#, t),
            Some(PedidoDeVoz::Parar)
        );
        assert_eq!(
            pedido_de_voz(r#"{"action":"speak","gen":7,"text":"oi","__t":"abc123"}"#, t),
            Some(PedidoDeVoz::Falar { geracao: 7, texto: "oi".into() })
        );
        assert_eq!(
            pedido_de_voz(r#"{"action":"speak","text":"oi","__t":"abc123"}"#, t),
            Some(PedidoDeVoz::Falar { geracao: 0, texto: "oi".into() })
        );
        // Ação desconhecida com token válido também não vira nada.
        assert_eq!(pedido_de_voz(r#"{"action":"exec","__t":"abc123"}"#, t), None);
    }

    /// O arquivo sem o módulo de teste — ver o comentário dentro da régua abaixo.
    fn so_o_codigo(fonte: &str) -> &str {
        fonte.split("#[cfg(test)]").next().unwrap_or(fonte)
    }

    /// Só existe UM jeito de criar janela do ShvIA — achado F-15 da revisão de 01/09/2026.
    ///
    /// As janelas de `target=_blank` nasciam de um builder próprio, sem `on_navigation`: um
    /// link externo clicado ali **navegava dentro do app**, e um site de terceiro passava a
    /// ocupar uma janela intitulada "ShvIA". Perímetro de phishing, não de FS — as pontes
    /// nativas também não eram instaladas.
    ///
    /// A régua persegue a causa, não o sintoma: duas janelas com regras diferentes é a mesma
    /// forma de defeito do `containerDo` (E-5) e dos dois caminhos de atualização (G-21) — a
    /// segunda cópia não nasce errada, ela envelhece sozinha. Se alguém acrescentar um
    /// `WebviewWindowBuilder::new` fora do `build_shvia_window`, isto falha.
    #[test]
    fn so_ha_uma_construcao_de_janela() {
        // 🐛 Corta o módulo de teste ANTES de contar: a primeira versão desta régua
        // contava os próprios literais dela (o `matches(...)` e a mensagem de erro) e
        // acusava 3. Régua que se conta acusa o arquivo certo pelo motivo errado — o
        // mesmo tropeço da prova de imagens no WORKSPACE, que acusou `node:fs`.
        let fonte = so_o_codigo(include_str!("lib.rs"));
        // Comentário cita o nome para explicar o achado; declaração é o que conta.
        let codigo: String = fonte
            .lines()
            .filter(|l| !l.trim_start().starts_with("//") && !l.trim_start().starts_with("///"))
            .collect::<Vec<_>>()
            .join("\n");
        let n = codigo.matches("WebviewWindowBuilder::new").count();
        assert_eq!(
            n, 1,
            "esperava 1 `WebviewWindowBuilder::new` (dentro de `build_shvia_window`), achei {n}. \
             Toda janela tem de sair da mesma função, senão ela nasce sem `on_navigation`."
        );
    }

    /// O handler de `target=_blank` chama a função canônica, e não constrói sozinho.
    #[test]
    fn o_target_blank_passa_pela_funcao_canonica() {
        let fonte = so_o_codigo(include_str!("lib.rs"));
        let i = fonte.find(".on_new_window(").expect("o handler de target=_blank sumiu");
        let trecho = &fonte[i..(i + 1400).min(fonte.len())];
        assert!(
            trecho.contains("build_shvia_window(&win_handle"),
            "o `on_new_window` voltou a construir a janela por fora",
        );
    }


    /// Todo handler nativo registrado passa por uma porta de token — achado F-16.
    ///
    /// A régua persegue a causa, não o sintoma. O `shviaCode` já checava o token desde a
    /// F-12; o `shviaTts` foi registrado depois, no mesmo `user_content_manager`, e ficou
    /// de fora — não porque alguém decidiu que voz é inofensiva, mas porque **a segunda
    /// ponte não herda a porta da primeira**. É a mesma forma de defeito do `containerDo`
    /// (E-5) e das duas construções de janela (F-15): a cópia não nasce errada, ela nasce
    /// sem a regra.
    ///
    /// Então a asserção não é "o shviaTts checa o token" — é **quantos handlers existem
    /// contra quantas portas existem**. Registrar um terceiro `messageHandler` sem gate
    /// deixa isto vermelho no `cargo test`, antes de virar superfície.
    #[test]
    fn todo_handler_nativo_tem_porta_de_token() {
        let fonte = so_o_codigo(include_str!("lib.rs"));
        let handlers = fonte.matches("register_script_message_handler(").count();
        // As duas portas: `pedido_de_voz` (voz) e `code_bridge::handle_message` (Modo
        // Code) — as duas comparam contra `bridge_token()` antes de agir.
        // Contamos CHAMADAS, não definições: `fn pedido_de_voz(` também casa com
        // `pedido_de_voz(` e faria a régua acusar uma porta a mais do que existe —
        // foi o que ela fez na primeira execução.
        let chamadas = |agulha: &str| fonte.matches(agulha).count()
            - fonte.matches(&format!("fn {agulha}")).count();
        let portas = chamadas("pedido_de_voz(") + chamadas("code_bridge::handle_message(");
        assert_eq!(
            handlers, portas,
            "{handlers} handler(s) nativo(s) registrado(s) para {portas} porta(s) de token — \
             o handler novo precisa checar o token antes de agir (ver pedido_de_voz)"
        );
    }

    /// `AGENTS.md` e `CLAUDE.md` são o mesmo texto abaixo do H1 — achado F-21.
    ///
    /// Os dois arquivos JÁ exigiam isso, por escrito, de si mesmos. E os dois violavam:
    /// no DESKTOP o comentário HTML do topo, no MOBILE o blockquote "Leia também". Não por
    /// desleixo — o bloco que divergia era exatamente o que dizia *"este arquivo é espelho
    /// do outro"*, e escrito em 1ª pessoa ele **não pode** ser idêntico nos dois. A regra
    /// era impossível de cumprir, o que é a razão pela qual instrução sem guarda apodrece:
    /// ninguém percebe que está pedindo o impossível.
    ///
    /// O ponteiro foi reescrito na 3ª pessoa (nomeia os dois arquivos, não "este"), e agora
    /// a régua vale de verdade. Um `diff` de uma linha, que roda a cada `cargo test`.
    #[test]
    fn agents_e_claude_sao_espelho() {
        let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        // Nos dois repos Tauri o Cargo.toml vive em `src-tauri/`; no CODE, na raiz.
        let base = if raiz.join("../AGENTS.md").exists() { raiz.join("..") } else { raiz.to_path_buf() };
        let ler = |n: &str| {
            let p = base.join(n);
            let t = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{} ilegível: {e}", p.display()));
            // Fora o H1 (a única linha que PODE diferir: cada arquivo tem seu título).
            t.lines().skip(1).collect::<Vec<_>>().join("\n")
        };
        let agents = ler("AGENTS.md");
        let claude = ler("CLAUDE.md");
        assert!(agents.len() > 500, "AGENTS.md tem {} bytes abaixo do H1 — a régua estaria medindo o vazio", agents.len());
        if agents != claude {
            let a: Vec<&str> = agents.lines().collect();
            let c: Vec<&str> = claude.lines().collect();
            let primeira = (0..a.len().max(c.len()))
                .find(|&i| a.get(i) != c.get(i))
                .map(|i| format!("linha {} abaixo do H1:\n    AGENTS.md: {:?}\n    CLAUDE.md: {:?}",
                    i + 2, a.get(i).unwrap_or(&"<fim>"), c.get(i).unwrap_or(&"<fim>")))
                .unwrap_or_default();
            panic!("AGENTS.md e CLAUDE.md divergem abaixo do H1 — {primeira}");
        }
    }

    /// Toda `uses:` do CI está pinada por SHA de commit — achado F-09.
    ///
    /// Tag de GitHub Action é **ponteiro móvel**: `actions/checkout@v4` roda o que o dono
    /// do repositório publicar amanhã sob aquela tag, com as permissões deste workflow e
    /// acesso ao token do job. Não é hipótese remota — é o vetor de `tj-actions/changed-files`
    /// (03/2025), em que uma tag movida passou a vazar segredos de milhares de repositórios.
    ///
    /// A régua persegue a causa, não o sintoma: o defeito não é "esta action está solta", é
    /// **uma action nova entrar sem pin**. Por isso ela varre o diretório inteiro e não uma
    /// lista — um workflow novo já nasce medido.
    ///
    /// O comentário `# vX.Y.Z` ao lado do SHA não é enfeite: sem ele, subir o pin vira
    /// arqueologia. A régua exige os dois.
    #[test]
    fn toda_action_do_ci_esta_pinada_por_sha() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github/workflows");
        let mut vistos = 0usize;
        let mut soltas: Vec<String> = Vec::new();

        let entradas = std::fs::read_dir(&dir).expect("o repositório tem .github/workflows");
        for e in entradas.flatten() {
            let caminho = e.path();
            let ext = caminho.extension().and_then(|x| x.to_str()).unwrap_or("");
            if ext != "yml" && ext != "yaml" {
                continue;
            }
            let arquivo = caminho.file_name().unwrap().to_string_lossy().to_string();
            let texto = std::fs::read_to_string(&caminho).expect("workflow legível");
            for (n, linha) in texto.lines().enumerate() {
                let corte = linha.trim_start();
                // Só `uses:` de action; `uses:` dentro de comentário não conta.
                if corte.starts_with('#') {
                    continue;
                }
                let Some(resto) = corte.strip_prefix("- uses:").or_else(|| corte.strip_prefix("uses:")) else {
                    continue;
                };
                let referencia = resto.trim().split('#').next().unwrap_or("").trim();
                // `uses:` local (`./algo`) e de container (`docker://`) não têm SHA a pinar.
                if referencia.starts_with('.') || referencia.starts_with("docker://") {
                    continue;
                }
                vistos += 1;
                let sha = referencia.rsplit('@').next().unwrap_or("");
                let pinada = sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit());
                // O comentário que diz QUAL versão o SHA é.
                let tem_nota = linha.contains('#');
                if !pinada || !tem_nota {
                    let porque = if pinada { "sem o comentário dizendo a versão" } else { "não é SHA de 40 hex" };
                    soltas.push(format!("{arquivo}:{}  {referencia}  ({porque})", n + 1));
                }
            }
        }

        assert!(vistos > 0, "nenhuma `uses:` encontrada — a régua estaria medindo o vazio");
        assert!(
            soltas.is_empty(),
            "action(s) do CI sem pin por SHA (+ comentário da versão):\n  {}\n\
             Resolva a tag com:\n  \
             gh api repos/<dono>/<repo>/git/ref/tags/<tag> --jq '.object.sha'",
            soltas.join("\n  ")
        );
    }

    /// Todo `.md` de `docs/` é alcançável por link a partir do índice — achado D-DOC-10.
    ///
    /// Documento órfão não é doc velha: é doc que **ninguém sabe que existe**. O
    /// `docs/loja-ficha.md` do MOBILE é a ficha do App Store Connect pronta para colar, com
    /// os limites de caracteres da Apple anotados — escrita em 04/08 e nunca linkada, num
    /// repo cuja submissão está pendente. O custo de um órfão não é o arquivo; é alguém
    /// reescrever o que já estava pronto.
    ///
    /// A régua não julga se a doc está atualizada — julga se dá para CHEGAR nela. Um `.md`
    /// novo em `docs/` deixa o `cargo test` vermelho até alguém decidir onde ele entra no
    /// índice, que é a decisão que ninguém toma quando o arquivo simplesmente aparece.
    #[test]
    fn todo_doc_e_alcancavel() {
        let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let base = if raiz.join("../docs").is_dir() { raiz.join("..") } else { raiz.to_path_buf() };
        let docs = base.join("docs");
        if !docs.is_dir() {
            return; // repo sem `docs/` não tem o que medir
        }

        // Os arquivos que podem CONTER links para os docs: tudo que é índice aqui.
        let mut indice = String::new();
        for n in ["README.md", "README_br.md", "docs/README.md", "CLAUDE.md", "AGENTS.md"] {
            if let Ok(t) = std::fs::read_to_string(base.join(n)) {
                indice.push_str(&t);
            }
        }
        // E os próprios docs linkam entre si — um doc alcançado por outro doc conta.
        let mut arquivos: Vec<std::path::PathBuf> = Vec::new();
        fn anda(d: &std::path::Path, saida: &mut Vec<std::path::PathBuf>) {
            if let Ok(e) = std::fs::read_dir(d) {
                for x in e.flatten() {
                    let p = x.path();
                    if p.is_dir() {
                        anda(&p, saida);
                    } else if p.extension().and_then(|s| s.to_str()) == Some("md") {
                        saida.push(p);
                    }
                }
            }
        }
        anda(&docs, &mut arquivos);
        for f in &arquivos {
            if let Ok(t) = std::fs::read_to_string(f) {
                indice.push_str(&t);
            }
        }

        let orfaos: Vec<String> = arquivos
            .iter()
            .filter_map(|f| {
                let rel = f.strip_prefix(&base).ok()?.to_string_lossy().to_string();
                let nome = f.file_name()?.to_string_lossy().to_string();
                // `docs/README.md` é o índice: ele não precisa ser linkado por ninguém.
                if rel == "docs/README.md" {
                    return None;
                }
                // Alcançável se alguém escreve um LINK markdown que termine no caminho ou
                // no nome do arquivo — `](docs/x.md)`, `](x.md)`, `](../docs/x.md)`.
                let alvo_a = format!("]({rel})");
                let alvo_b = format!("]({nome})");
                let alvo_c = format!("/{nome})");
                let visto = indice.contains(&alvo_a) || indice.contains(&alvo_b) || indice.contains(&alvo_c);
                (!visto).then_some(rel)
            })
            .collect();

        assert!(!arquivos.is_empty(), "docs/ vazio — a régua estaria medindo o vazio");
        assert!(
            orfaos.is_empty(),
            "documento(s) em docs/ sem NENHUM link apontando para eles:\n  {}\n\
             Um .md que ninguém alcança é trabalho que alguém vai refazer. Linke no índice \
             (README.md ou docs/README.md) ou apague.",
            orfaos.join("\n  ")
        );
    }

}
