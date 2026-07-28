//! "Diagnóstico" do app (item **D7**; ADR-025) — `Ajuda → Diagnóstico…`.
//!
//! ## A diferença entre EXIBIR e VERIFICAR
//!
//! O modal "Sobre" já reunia build, versão do servidor, host e WebView, com botão
//! Copiar. Mas ele **exibe**: mostra o host configurado e não diz se aquele host
//! responde; mostra a versão e não diz se ela consegue se atualizar.
//!
//! Todo item aqui tem **veredito**, e cada um existe porque corresponde a uma falha que
//! hoje acontece **em silêncio** — o app não erra, simplesmente não faz:
//!
//! | Item | Como falha hoje, sem este painel |
//! |---|---|
//! | Servidor alcançável | janela abre e fica em branco, ou a tarja offline aparece "sem motivo" |
//! | Permissão de notificação | o alerta de preço **nunca** chega, e nada acusa (ADR-011) |
//! | Motor `anna` | o Modo Code não responde e parece travado (D5) |
//! | Rodando de dentro do DMG | o auto-update falha para sempre; a preferência não persiste |
//! | Bandeja criada | com o D2 ligado, fechar a janela faria o app **desaparecer** |
//!
//! Os três primeiros são de longe as perguntas mais frequentes de suporte, e todas as
//! três hoje exigem alguém com terminal.
//!
//! ## Por que o relatório copiável é metade do item
//!
//! Diagnóstico que o usuário tem de **transcrever** é diagnóstico que chega errado. O
//! botão Copiar produz um texto que cabe numa mensagem — é a razão de existir o painel
//! para quem não vai consertar nada sozinho.
//!
//! ## Nada de comando novo exposto à página
//!
//! O painel é `eval` de um modal autocontido, com os dados já resolvidos no Rust e
//! embutidos como JSON — o mesmo desenho do modal "Sobre". O [ADR-001] segue valendo:
//! um servidor comprometido não ganha um `invoke` que enumera caminhos de arquivo do
//! host.
//!
//! [ADR-001]: ../../docs/decisoes.md

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;

/// Veredito de um item. Três níveis, e a distinção importa: `aviso` é o que **pode**
/// atrapalhar (e às vezes é escolha do usuário), `falha` é o que **está** quebrado.
/// Colapsar os dois faria o painel gritar em situação normal, e painel que sempre grita
/// é painel que ninguém lê.
#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Veredito {
    Ok,
    Aviso,
    Falha,
}

#[derive(Serialize, Clone, Debug)]
pub struct Item {
    pub rotulo: &'static str,
    pub veredito: Veredito,
    /// O que foi encontrado (vai para a tela e para o relatório).
    pub valor: String,
    /// O que fazer. Vazio quando não há nada a fazer.
    pub dica: String,
}

impl Item {
    fn novo(rotulo: &'static str, veredito: Veredito, valor: impl Into<String>, dica: impl Into<String>) -> Self {
        Self { rotulo, veredito, valor: valor.into(), dica: dica.into() }
    }
}

/// Roda as verificações.
///
/// **Nunca lança e nunca bloqueia por muito tempo:** o único item que toca a rede é o
/// probe do servidor, que já tem timeout curto próprio. Um diagnóstico que trava é pior
/// que nenhum — quem o abre já está com o app se comportando mal.
pub fn coletar(app: &AppHandle) -> Vec<Item> {
    let mut itens = Vec::new();

    // ── 1. Servidor ────────────────────────────────────────────────────────────
    let cfg = crate::server::load(app);
    let alcancavel = crate::server::probe(&cfg.url);
    itens.push(Item::novo(
        "Servidor",
        if alcancavel { Veredito::Ok } else { Veredito::Falha },
        &cfg.url,
        if alcancavel {
            String::new()
        } else {
            "Não consegui abrir conexão. Confira a internet, a VPN e o endereço em Ajuda → Servidor.".into()
        },
    ));

    // ── 2. Permissão de notificação ────────────────────────────────────────────
    // A mais silenciosa de todas: negada, o alerta de preço do ADR-011 nunca aparece e
    // NADA acusa — nem log, nem erro. O usuário conclui que o rastreador não funciona.
    let permissao = app
        .notification()
        .permission_state()
        .map(|p| format!("{p:?}"))
        .unwrap_or_else(|_| "desconhecida".into());
    let negada = permissao.eq_ignore_ascii_case("denied");
    itens.push(Item::novo(
        "Notificações do sistema",
        if negada { Veredito::Falha } else { Veredito::Ok },
        if negada { "bloqueadas".to_string() } else { "liberadas".to_string() },
        if negada {
            "Os alertas de preço não vão aparecer. Libere em Ajustes do sistema → Notificações → ShvIA.".into()
        } else {
            String::new()
        },
    ));

    // ── 3. Motor do Modo Code (item D5) ────────────────────────────────────────
    let motor = crate::code_bridge::engine_status("anna");
    let achou = motor.get("found").and_then(|v| v.as_bool()).unwrap_or(false);
    let empacotado = motor.get("bundled").and_then(|v| v.as_bool()).unwrap_or(false);
    let versao = motor.get("version").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let caminho = motor.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();

    // `found` e `version` são separados de propósito (D5): um `anna` que existe mas não
    // responde `--version` é binário quebrado, e é diferente de binário ausente.
    let (v_motor, valor_motor, dica_motor) = match (achou, versao.is_empty()) {
        (false, _) => (
            Veredito::Aviso,
            "não encontrado".to_string(),
            "O Modo Code não vai funcionar. Instalações a partir da 1.0.0 já trazem o motor; reinstale o app.".to_string(),
        ),
        (true, true) => (
            Veredito::Falha,
            format!("encontrado, mas sem responder ({caminho})"),
            "O binário existe e não respondeu `--version` — provavelmente está corrompido ou sem permissão de execução.".to_string(),
        ),
        (true, false) => (
            Veredito::Ok,
            format!("{versao}{}", if empacotado { " (do instalador)" } else { " (do sistema)" }),
            String::new(),
        ),
    };
    itens.push(Item::novo("Motor do Modo Code", v_motor, valor_motor, dica_motor));

    // ── 4. De onde o app está rodando ──────────────────────────────────────────
    // Falha real e comum no macOS: arrastar o app do DMG não é obrigatório para abrir,
    // então muita gente roda direto do volume montado. Dali o auto-update não consegue
    // substituir o bundle (volume somente-leitura) e a preferência não persiste — e o
    // sintoma é "a atualização nunca instala", que não aponta para a causa.
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
    let no_dmg = exe.starts_with("/Volumes/");
    let fora_de_applications = cfg!(target_os = "macos") && !no_dmg && !exe.contains("/Applications/");
    let (v_local, dica_local) = if no_dmg {
        (
            Veredito::Falha,
            "O app está rodando de dentro do instalador (DMG). Arraste o ShvIA para a pasta Aplicativos e abra de lá — daqui a atualização automática não consegue funcionar.".to_string(),
        )
    } else if fora_de_applications {
        (
            Veredito::Aviso,
            "Fora da pasta Aplicativos. Funciona, mas a atualização automática pode falhar por permissão de escrita.".to_string(),
        )
    } else {
        (Veredito::Ok, String::new())
    };
    itens.push(Item::novo("Local de instalação", v_local, exe, dica_local));

    // ── 5. Bandeja (item D2) ───────────────────────────────────────────────────
    // A interação perigosa do D2: em ambiente sem host de bandeja (algumas sessões
    // GNOME), o ícone não aparece. Com "fechar mantém rodando" ligado, fechar a janela
    // faria o app DESAPARECER sem jeito de voltar. Por isso o `lib.rs` desliga a
    // preferência quando a criação falha — e por isso este item reporta as duas coisas
    // juntas.
    let tem_bandeja = app.tray_by_id(crate::tray::TRAY_ID).is_some();
    let prefs = crate::tray::ler(app);
    let (v_bandeja, dica_bandeja) = match (tem_bandeja, prefs.close_to_tray) {
        (false, true) => (
            Veredito::Falha,
            "Sem bandeja e com \"fechar mantém rodando\" ligado, fechar a janela esconderia o app sem volta. Desligue a opção no menu da bandeja — ou, se o ícone não aparece, instale o suporte a bandeja do seu ambiente.".to_string(),
        ),
        (false, false) => (
            Veredito::Aviso,
            "O ícone não foi criado. O app funciona, mas fechar a janela encerra — e o alerta de preço só chega com o app aberto.".to_string(),
        ),
        (true, _) => (Veredito::Ok, String::new()),
    };
    itens.push(Item::novo(
        "Bandeja / menubar",
        v_bandeja,
        if tem_bandeja { "ativa" } else { "não criada" },
        dica_bandeja,
    ));

    // ── 6. Estado das preferências (informativo) ───────────────────────────────
    itens.push(Item::novo(
        "Fechar mantém rodando",
        Veredito::Ok,
        if prefs.close_to_tray { "sim" } else { "não" },
        String::new(),
    ));
    itens.push(Item::novo(
        "Iniciar com o sistema",
        Veredito::Ok,
        if app.autolaunch().is_enabled().unwrap_or(false) { "sim" } else { "não" },
        String::new(),
    ));

    itens
}

/// Abre o painel na janela em foco.
pub fn abrir(app: &AppHandle) {
    let itens = coletar(app);

    // `to_string` do serde escapa `"`, `\` e controles. Os dois separadores de linha
    // Unicode ficam de fora dessa lista e são terminadores de linha em JS antigo —
    // escapamos à mão para o `eval` não quebrar com um caminho de arquivo exótico.
    let json = serde_json::to_string(&itens)
        .unwrap_or_else(|_| "[]".into())
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");

    let js = MODAL_JS
        .replace("__SHVIA_DIAG__", &json)
        .replace("__SHVIA_BUILD__", &app.package_info().version.to_string())
        .replace("__SHVIA_OS__", std::env::consts::OS);

    let janelas = app.webview_windows();
    let alvo = janelas
        .values()
        .find(|w| w.is_focused().unwrap_or(false))
        .or_else(|| janelas.values().next());

    if let Some(win) = alvo {
        // Sem isto, o painel abriria invisível numa janela minimizada e o menu pareceria
        // morto — mesma correção que o modal "Sobre" precisou.
        let _ = win.unminimize();
        let _ = win.set_focus();
        let _ = win.eval(&js);
    }
}

/// Modal do diagnóstico. ES5, autocontido, sem IPC — mesmo padrão do modal "Sobre",
/// e reaproveita as variáveis de tema do ShvIA com fallback para quando o painel abre
/// sobre a casca local (que não tem o CSS do servidor).
const MODAL_JS: &str = r#"(function () {
  var DADOS = __SHVIA_DIAG__;
  var BUILD = '__SHVIA_BUILD__';
  var SO = '__SHVIA_OS__';

  var old = document.getElementById('shvia-diag-overlay');
  var prevFocus = (old && old.__shviaPrevFocus) || document.activeElement;
  if (old) { old.remove(); }

  var css = [
    '#shvia-diag-overlay{position:fixed;top:0;right:0;bottom:0;left:0;z-index:2147483600;display:flex;align-items:center;justify-content:center;',
    'background:rgba(3,8,14,.62);opacity:0;transition:opacity .16s ease-out}',
    '#shvia-diag-overlay.shvia-diag-on{opacity:1}',
    '.shvia-diag-card{width:min(540px,calc(100vw - 40px));max-height:calc(100vh - 60px);overflow:auto;box-sizing:border-box;padding:28px;',
    'background:var(--bg-3,#131A26);border:1px solid var(--line-2,rgba(150,170,200,.18));',
    'border-radius:var(--radius-md,16px);box-shadow:var(--shadow-lg,0 14px 40px rgba(3,8,14,.4));',
    'color:var(--tx,#ECF1F8);font-family:var(--font-sans,system-ui,sans-serif);',
    'transform:scale(.97);opacity:0;transition:transform .2s cubic-bezier(.22,1,.36,1),opacity .2s ease-out}',
    '#shvia-diag-overlay.shvia-diag-on .shvia-diag-card{transform:scale(1);opacity:1}',
    '.shvia-diag-title{margin:0;font:600 1.25rem/1.3 var(--font-display,system-ui,sans-serif);letter-spacing:-.01em}',
    '.shvia-diag-sub{margin:4px 0 20px;font-size:.8rem;color:var(--tx-dim,#97A3B5)}',
    '.shvia-diag-item{display:grid;grid-template-columns:14px 1fr;gap:4px 12px;padding:12px 0;border-top:1px solid var(--line-1,rgba(150,170,200,.10))}',
    '.shvia-diag-dot{width:8px;height:8px;margin-top:6px;border-radius:50%}',
    '.shvia-diag-ok .shvia-diag-dot{background:#3DD68C}',
    '.shvia-diag-aviso .shvia-diag-dot{background:#E8B339}',
    '.shvia-diag-falha .shvia-diag-dot{background:#F2555A}',
    '.shvia-diag-rot{margin:0;font:500 .68rem/1.4 var(--font-mono,Consolas,monospace);letter-spacing:.08em;text-transform:uppercase;color:var(--tx-dim,#97A3B5)}',
    '.shvia-diag-val{margin:2px 0 0;grid-column:2;font:500 .85rem/1.45 var(--font-mono,Consolas,monospace);overflow-wrap:anywhere}',
    '.shvia-diag-dica{margin:6px 0 0;grid-column:2;font:400 .78rem/1.5 var(--font-sans,system-ui,sans-serif);color:var(--tx-dim,#97A3B5)}',
    '.shvia-diag-actions{display:flex;justify-content:flex-end;gap:10px;margin-top:22px}',
    '.shvia-diag-actions button{font:500 .8rem/1 var(--font-sans,system-ui,sans-serif);padding:9px 16px;border-radius:10px;cursor:pointer;transition:background .15s ease-out,color .15s ease-out}',
    '.shvia-diag-actions button:focus-visible{outline:2px solid var(--accent,#34B3EC);outline-offset:2px}',
    '#shvia-diag-copy{background:transparent;border:1px solid var(--line-2,rgba(150,170,200,.18));color:var(--tx-dim,#97A3B5)}',
    '#shvia-diag-copy:hover{background:var(--bg-item-hover,rgba(255,255,255,.05));color:var(--tx,#ECF1F8)}',
    '#shvia-diag-close{background:var(--azure-soft,rgba(52,179,236,.12));border:1px solid rgba(52,179,236,.35);color:var(--azure-strong,#5FC8F5)}',
    '#shvia-diag-close:hover{background:rgba(52,179,236,.22)}',
    '@media (prefers-reduced-motion:reduce){#shvia-diag-overlay,.shvia-diag-card,.shvia-diag-actions button{transition:none}}'
  ].join('');
  var style = document.getElementById('shvia-diag-style');
  if (!style) {
    style = document.createElement('style');
    style.id = 'shvia-diag-style';
    (document.head || document.documentElement).appendChild(style);
  }
  style.textContent = css;

  var overlay = document.createElement('div');
  overlay.id = 'shvia-diag-overlay';
  var card = document.createElement('div');
  card.className = 'shvia-diag-card';
  card.setAttribute('role', 'dialog');
  card.setAttribute('aria-modal', 'true');
  card.setAttribute('aria-labelledby', 'shvia-diag-title');

  var problemas = 0;
  for (var i = 0; i < DADOS.length; i++) {
    if (DADOS[i].veredito !== 'ok') { problemas++; }
  }

  var h = document.createElement('h2');
  h.className = 'shvia-diag-title';
  h.id = 'shvia-diag-title';
  h.textContent = 'Diagnóstico';
  card.appendChild(h);

  var sub = document.createElement('p');
  sub.className = 'shvia-diag-sub';
  // O resumo primeiro: quem abre isto quer saber "tem algo errado?" antes de ler sete
  // linhas. Sem ele, o painel obriga o usuário a auditar a lista para descobrir.
  sub.textContent = problemas === 0
    ? 'Tudo em ordem — ' + DADOS.length + ' verificações, nenhum problema.'
    : (problemas === 1 ? '1 ponto de atenção' : problemas + ' pontos de atenção') + ' em ' + DADOS.length + ' verificações.';
  card.appendChild(sub);

  for (var j = 0; j < DADOS.length; j++) {
    var it = DADOS[j];
    var box = document.createElement('div');
    box.className = 'shvia-diag-item shvia-diag-' + it.veredito;

    var dot = document.createElement('span');
    dot.className = 'shvia-diag-dot';
    // O ponto é decorativo; quem carrega o significado para leitor de tela é o texto do
    // rótulo, abaixo — cor sozinha nunca é informação.
    dot.setAttribute('aria-hidden', 'true');
    box.appendChild(dot);

    var rot = document.createElement('p');
    rot.className = 'shvia-diag-rot';
    var prefixo = it.veredito === 'ok' ? '' : (it.veredito === 'falha' ? 'problema · ' : 'atenção · ');
    rot.textContent = prefixo + it.rotulo;
    box.appendChild(rot);

    var val = document.createElement('p');
    val.className = 'shvia-diag-val';
    // textContent sempre: os valores incluem caminho de arquivo e host, e nada disso
    // pode virar markup.
    val.textContent = it.valor;
    box.appendChild(val);

    if (it.dica) {
      var d = document.createElement('p');
      d.className = 'shvia-diag-dica';
      d.textContent = it.dica;
      box.appendChild(d);
    }
    card.appendChild(box);
  }

  var actions = document.createElement('div');
  actions.className = 'shvia-diag-actions';
  var copiar = document.createElement('button');
  copiar.type = 'button';
  copiar.id = 'shvia-diag-copy';
  copiar.setAttribute('aria-live', 'polite');
  copiar.textContent = 'Copiar';
  var fechar = document.createElement('button');
  fechar.type = 'button';
  fechar.id = 'shvia-diag-close';
  fechar.textContent = 'Fechar';
  actions.appendChild(copiar);
  actions.appendChild(fechar);
  card.appendChild(actions);

  overlay.appendChild(card);
  overlay.__shviaPrevFocus = prevFocus;
  (document.body || document.documentElement).appendChild(overlay);

  function relatorio() {
    var l = ['ShvIA Desktop ' + BUILD + ' (' + SO + ') — diagnóstico', ''];
    for (var k = 0; k < DADOS.length; k++) {
      var x = DADOS[k];
      var marca = x.veredito === 'ok' ? '[ok]  ' : (x.veredito === 'falha' ? '[ERRO]' : '[!]   ');
      l.push(marca + ' ' + x.rotulo + ': ' + x.valor);
      if (x.dica) { l.push('        -> ' + x.dica); }
    }
    return l.join('\n');
  }

  copiar.addEventListener('click', function () {
    var txt = relatorio();
    function feito() {
      copiar.textContent = 'Copiado';
      setTimeout(function () { copiar.textContent = 'Copiar'; }, 1600);
    }
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(txt).then(feito, fallback);
    } else {
      fallback();
    }
    // WebKitGTK sem permissão de clipboard e contextos não-seguros recusam a API nova.
    // Sem este caminho, o botão que É metade do item não faria nada em um dos três SOs.
    function fallback() {
      var ta = document.createElement('textarea');
      ta.value = txt;
      ta.setAttribute('readonly', 'readonly');
      ta.style.position = 'fixed';
      ta.style.opacity = '0';
      (document.body || document.documentElement).appendChild(ta);
      ta.select();
      try { document.execCommand('copy'); feito(); } catch (e) { copiar.textContent = 'Selecione e copie'; }
      ta.remove();
    }
  });

  function sair() {
    overlay.classList.remove('shvia-diag-on');
    document.removeEventListener('keydown', onKey, true);
    setTimeout(function () {
      overlay.remove();
      if (prevFocus && prevFocus.focus) { try { prevFocus.focus(); } catch (e) {} }
    }, 180);
  }
  function onKey(e) {
    if (e.key === 'Escape') { e.preventDefault(); sair(); return; }
    // Foco preso no diálogo: sem isto o Tab levaria para a página atrás, que continua
    // interativa por baixo do overlay.
    if (e.key === 'Tab') {
      var f = [copiar, fechar];
      var i2 = f.indexOf(document.activeElement);
      e.preventDefault();
      var prox = e.shiftKey ? (i2 <= 0 ? f.length - 1 : i2 - 1) : (i2 >= f.length - 1 ? 0 : i2 + 1);
      f[prox].focus();
    }
  }
  fechar.addEventListener('click', sair);
  overlay.addEventListener('click', function (e) { if (e.target === overlay) { sair(); } });
  document.addEventListener('keydown', onKey, true);

  requestAnimationFrame(function () {
    overlay.classList.add('shvia-diag-on');
    fechar.focus();
  });
})();"#;

#[cfg(test)]
mod tests {
    use super::{Item, Veredito};

    /// O JSON dos itens é embutido no `eval`. Se o `serde` deixasse de escapar aspas, um
    /// caminho de arquivo com `"` no nome fecharia a string e o modal viraria sintaxe
    /// inválida — o painel simplesmente não abriria, sem erro visível.
    #[test]
    fn valor_com_aspas_e_escapado_no_json() {
        let itens = vec![Item::novo("Local", Veredito::Ok, r#"/tmp/pasta "com" aspas"#, "")];
        let json = serde_json::to_string(&itens).expect("serializa");

        assert!(json.contains(r#"\"com\""#), "aspas precisam sair escapadas: {json}");
        assert!(!json.contains(r#""com""#), "aspas cruas quebrariam o eval");
    }

    /// O veredito vai para o CSS como `shvia-diag-<veredito>`, então a grafia é contrato
    /// entre Rust e JS. Mudar para `Ok`/`OK` deixaria a bolinha sem cor e o resumo
    /// contando problema onde não há.
    #[test]
    fn o_veredito_serializa_em_minusculas() {
        let json = serde_json::to_string(&vec![
            Item::novo("a", Veredito::Ok, "", ""),
            Item::novo("b", Veredito::Aviso, "", ""),
            Item::novo("c", Veredito::Falha, "", ""),
        ])
        .expect("serializa");

        assert!(json.contains(r#""veredito":"ok""#));
        assert!(json.contains(r#""veredito":"aviso""#));
        assert!(json.contains(r#""veredito":"falha""#));
    }

    /// Todo placeholder do modal precisa de um `.replace` correspondente em `abrir`.
    /// Esquecer um faz a string literal (`__SHVIA_OS__`) vazar para a tela do usuário —
    /// o mesmo risco que o docblock do modal "Sobre" já registrava.
    #[test]
    fn todo_placeholder_do_modal_e_substituido() {
        let substituidos = ["__SHVIA_DIAG__", "__SHVIA_BUILD__", "__SHVIA_OS__"];

        let mut js = super::MODAL_JS.to_string();
        for p in substituidos {
            js = js.replace(p, "x");
        }

        assert!(
            !js.contains("__SHVIA_"),
            "sobrou placeholder sem replace em diagnostico::abrir"
        );
    }
}
