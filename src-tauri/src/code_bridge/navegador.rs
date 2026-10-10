//! The agent's own browser on this machine (`navegador-local`, ADR-042): the second family of
//! device commands, behind its own tray switch. The agent asks, the person says yes in a native
//! dialog, and a window of its own opens, shows what the agent sees, and keeps the person's
//! logins in a profile that lives on this disk and nowhere else.
//!
//! Why a window of its own, and not the person's Chrome: the person watches it, closes it to
//! stop the agent, and signs in to the portal by hand, so a password never passes through the
//! agent or the model. Driving the real Chrome would need a debugging port on a browser that
//! holds every account the person has.
//!
//! Five commands, `browser.open`, `.read`, `.click`, `.type`, `.close`, and these gates, none of
//! them on the page:
//!
//! 1. The master switch of the device commands, then **this family's own switch** (tray,
//!    off by default, only a click there turns it on).
//! 2. A closed list, as in `aparelho.rs`.
//! 3. **Consent in a native dialog**: once per site for opening and reading (the grant lives until
//!    the window closes), and for EVERY click and EVERY typed text. No "always allow".
//! 4. A field that holds a secret is never typed into, whatever the person says: they type it.
//!
//! The window has no way back into the app. It gets no message handler (a ruler below counts), no
//! label the capabilities cover (another ruler), and every script it runs returns through
//! `eval_with_callback`, so a hostile page cannot call Rust, only lie about what it shows. What
//! comes back is rebuilt from known fields with caps (`sanear_leitura`) before it leaves for the
//! server, and the server treats it as untrusted text.

use super::aparelho::{falha, prazo_por_extenso, vencido};
use super::*;
use std::collections::HashSet;
use std::sync::{Condvar, LazyLock, Mutex};
use std::time::{Duration, Instant};
use tauri::webview::{DownloadEvent, NewWindowResponse, PageLoadEvent};
use tauri::{Url, WebviewUrl, WebviewWindowBuilder, WindowEvent};

/// Every command of this family, in the order `aparelhoStatus` lists them.
pub(crate) const COMANDOS: [&str; 5] =
    ["browser.open", "browser.read", "browser.click", "browser.type", "browser.close"];

/// The label of the agent's window. No capability covers it (a ruler reads `default.json`).
pub(super) const JANELA: &str = "agente-navegador";

/// The script the window runs; see its header for what it can and cannot be trusted with.
const JS: &str = include_str!("navegador.js");

/// How long a page may take to finish loading before the answer says it has not.
const PRAZO_CARGA: Duration = Duration::from_secs(25);
/// After a load, a short pause: most pages draw their content a moment after `load`.
const ASSENTAR: Duration = Duration::from_millis(700);
/// How long a click may take to start a navigation that the answer should wait for.
const PRAZO_NAVEGACAO: Duration = Duration::from_millis(2500);
/// How long one script may take to answer.
const PRAZO_SCRIPT: Duration = Duration::from_secs(10);
/// The most a script may hand back; a page can make its own values as large as it likes.
const MAX_RESPOSTA: usize = 400 * 1024;
/// The most text a single `browser.type` may carry.
const MAX_DIGITADO: usize = 500;
/// The caps `sanear_leitura` applies to what a page says.
const MAX_TEXTO: usize = 20_000;
const MAX_ELEMENTOS: usize = 150;
const MAX_ROTULO: usize = 120;
const MAX_DESTINO: usize = 200;

/// The data store of the agent's window on macOS, where `data_directory` does not exist.
/// Sixteen bytes, as `data_store_identifier` wants. (macOS 14 and later; older systems ignore it.)
#[cfg(target_os = "macos")]
const ID_DO_PERFIL: [u8; 16] = *b"shvia-agente-nav";

type Falha = (&'static str, String);

/// What this family remembers between commands, in memory only: it dies with the app and with
/// the window, so a grant never outlives the window the person can see.
struct Estado {
    /// Sites the person allowed in this window's lifetime (`host[:port]`).
    concedidos: HashSet<String>,
    /// How many page loads have finished; commands wait on a change of this number.
    cargas: u64,
    /// The generation of the last `browser.read`: the numbers it handed out are valid for it only.
    geracao: u64,
}

static ESTADO: LazyLock<(Mutex<Estado>, Condvar)> = LazyLock::new(|| {
    (Mutex::new(Estado { concedidos: HashSet::new(), cargas: 0, geracao: 0 }), Condvar::new())
});

fn estado() -> std::sync::MutexGuard<'static, Estado> {
    ESTADO.0.lock().unwrap_or_else(|e| e.into_inner())
}

fn carga_terminou() {
    estado().cargas += 1;
    ESTADO.1.notify_all();
}

/// Waits for a load to finish after the count was `desde`. `true` when one did.
fn esperar_carga(desde: u64, prazo: Duration) -> bool {
    let limite = Instant::now() + prazo;
    let mut e = estado();
    while e.cargas == desde {
        let Some(resto) = limite.checked_duration_since(Instant::now()) else {
            return false;
        };
        let (g, saiu) = ESTADO.1.wait_timeout(e, resto).unwrap_or_else(|p| p.into_inner());
        e = g;
        if saiu.timed_out() && e.cargas == desde {
            return false;
        }
    }
    true
}

/// The window is gone (the person closed it, or `browser.close` did): the grants and the numbers
/// the agent was holding go with it.
fn esquecer() {
    let mut e = estado();
    e.concedidos.clear();
    e.geracao = 0;
}

// ─── addresses ────────────────────────────────────────────────────────────────────────────────

/// An address the agent may open: `http` or `https`, with a host and no credentials in it.
/// `file:`, `javascript:` and the app's own `shvia:` never get this far, and
/// `https://bank.com@evil.example/` is the classic way to read a host wrongly.
fn url_permitida(bruta: &str) -> Result<Url, Falha> {
    let bruta = bruta.trim();
    if bruta.is_empty() || bruta.len() > 2000 {
        return Err(("argumento_invalido", "o endereço está vazio ou é longo demais".to_string()));
    }
    let url = Url::parse(bruta)
        .map_err(|_| ("argumento_invalido", "isso não é um endereço completo (use https://…)".to_string()))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err((
            "argumento_invalido",
            format!("só endereços http e https; recebi “{}”", limpo(url.scheme(), 20)),
        ));
    }
    if url.host_str().is_none() {
        return Err(("argumento_invalido", "o endereço não tem site".to_string()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(("argumento_invalido", "o endereço traz usuário e senha; o agente não abre isso".to_string()));
    }
    Ok(url)
}

/// `host` or `host:port` when the port is not the scheme's own, lowercased: the unit of a grant.
fn host_de(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default().to_lowercase();
    match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host,
    }
}

/// Is this address on the person's own network or machine, and not on the internet? The dialog
/// says so, because an agent that was talked into opening the router's page is a real case.
fn endereco_local(url: &Url) -> bool {
    let Some(h) = url.host_str() else { return false };
    let h = h.trim_start_matches('[').trim_end_matches(']').to_lowercase();
    if let Ok(ip) = h.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
            }
            std::net::IpAddr::V6(v6) => {
                let s = v6.segments()[0];
                v6.is_loopback() || v6.is_unspecified() || (s & 0xfe00) == 0xfc00 || (s & 0xffc0) == 0xfe80
            }
        };
    }
    h == "localhost"
        || h.ends_with(".localhost")
        || h.ends_with(".local")
        || h.ends_with(".lan")
        || h.ends_with(".internal")
        || !h.contains('.')
}

/// Navigation inside the window: only web pages. A link to `file:`, a custom scheme or a
/// `javascript:` address does nothing.
fn navegacao_permitida(u: &Url) -> bool {
    matches!(u.scheme(), "http" | "https") || u.as_str() == "about:blank"
}

// ─── what is shown to the person ──────────────────────────────────────────────────────────────

/// Text that came from the agent or from a page, made safe to quote in a dialog: one line, no
/// control characters, and none of the invisible characters that reorder text (an override can
/// make “Cancelar” read as something else), cut at `max` characters.
fn limpo(s: &str, max: usize) -> String {
    let sem_controle: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .filter(|c| !matches!(*c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}'))
        .collect();
    let junto = sem_controle.split_whitespace().collect::<Vec<_>>().join(" ");
    if junto.chars().count() > max {
        let mut cortado: String = junto.chars().take(max).collect();
        cortado.push('…');
        cortado
    } else {
        junto
    }
}

fn motivo_citado(motivo: &str) -> String {
    if motivo.is_empty() {
        "nenhum".to_string()
    } else {
        format!("“{motivo}”")
    }
}

fn texto_abrir(url: &Url, host: &str, motivo: &str, teto: Duration) -> String {
    let local = if endereco_local(url) {
        "\n\nAtenção: este endereço é da sua própria rede (ou desta máquina), não da internet."
    } else {
        ""
    };
    format!(
        "O agente do ShvIA quer abrir uma página no navegador dele, nesta máquina.\n\n\
         Endereço: {url}\n\
         Motivo, nas palavras do agente (não verificado): {motivo}\n\n\
         Se você deixar, o agente poderá abrir e ler as páginas de {host} nessa janela, até você \
         fechá-la. Cada clique e cada texto digitado serão perguntados à parte. Senhas você \
         digita sozinho: o agente não recebe o que você digita nos campos.\n\n\
         O texto das páginas lidas vai para a conversa, no servidor do ShvIA.{local}\n\n\
         O pedido vale por {prazo}.",
        url = limpo(url.as_str(), 300),
        motivo = motivo_citado(motivo),
        prazo = prazo_por_extenso(teto),
    )
}

fn texto_ler(host: &str, titulo: &str, motivo: &str, teto: Duration) -> String {
    format!(
        "O agente do ShvIA quer ler a página que está aberta no navegador dele.\n\n\
         Site: {host}\n\
         Título, como a própria página se descreve: “{titulo}”\n\
         Motivo, nas palavras do agente (não verificado): {motivo}\n\n\
         O texto visível da página e a lista dos botões e campos dela vão para a conversa, no \
         servidor do ShvIA. Se você deixar, o agente poderá ler também as outras páginas de \
         {host} nessa janela.\n\n\
         O pedido vale por {prazo}.",
        titulo = limpo(titulo, 100),
        motivo = motivo_citado(motivo),
        prazo = prazo_por_extenso(teto),
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Acao {
    Clicar,
    Digitar,
}

/// The element as the page describes it. Every string is the page's.
#[derive(Clone, Debug, Default, PartialEq)]
struct Descricao {
    tag: String,
    tipo: String,
    rotulo: String,
    destino: String,
    secreto: bool,
    editavel: bool,
}

impl Descricao {
    /// What kind of thing it is, in words a person uses.
    fn natureza(&self) -> &'static str {
        match (self.tag.as_str(), self.tipo.as_str()) {
            ("a", _) => "link",
            ("button", _) | ("input", "submit") | ("input", "button") | ("input", "reset") => "botão",
            ("input", "checkbox") | ("input", "radio") => "caixa de marcação",
            ("select", _) => "lista de opções",
            ("textarea", _) | ("input", _) => "campo de texto",
            ("summary", _) => "seção recolhível",
            _ => "elemento",
        }
    }

    fn por_extenso(&self) -> String {
        let nome = if self.rotulo.is_empty() { "sem nome".to_string() } else { format!("“{}”", self.rotulo) };
        let para = if self.destino.is_empty() { String::new() } else { format!(", que vai para {}", self.destino) };
        format!("{} {nome}{para}", self.natureza())
    }
}

fn texto_acao(acao: Acao, host: &str, desc: &Descricao, texto: Option<&str>, motivo: &str, teto: Duration) -> String {
    let (cabeca, rotulo_do_elemento, fecho) = match acao {
        Acao::Clicar => (
            "O agente do ShvIA quer CLICAR em um elemento da página.",
            "Elemento, como a página o descreve",
            "Um clique pode enviar um formulário, comprar, apagar ou publicar algo, conforme a \
             página. Continue só se foi isso que você pediu.",
        ),
        Acao::Digitar => (
            "O agente do ShvIA quer DIGITAR um texto em um campo da página.",
            "Campo, como a página o descreve",
            "O texto vai para o site, e não só para o ShvIA. Continue só se foi isso que você pediu.",
        ),
    };
    let linha_do_texto = match texto {
        Some(t) => format!("Texto: “{}”\n", limpo(t, MAX_DIGITADO)),
        None => String::new(),
    };
    format!(
        "{cabeca}\n\n\
         Site: {host}\n\
         {rotulo_do_elemento}: {elemento}\n\
         {linha_do_texto}\
         Motivo, nas palavras do agente (não verificado): {motivo}\n\n\
         {fecho}\n\n\
         O pedido vale por {prazo}.",
        elemento = desc.por_extenso(),
        motivo = motivo_citado(motivo),
        prazo = prazo_por_extenso(teto),
    )
}

/// Native confirmation, off the UI thread (`blocking_show` would deadlock the loop that has to
/// paint the dialog). `true` only on the first button.
fn perguntar(window: &WebviewWindow, texto: String, sim: &str) -> bool {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    window
        .app_handle()
        .dialog()
        .message(texto)
        .title("Navegador do agente")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(sim.into(), "Recusar".into()))
        .blocking_show()
}

// ─── what comes back from a page ──────────────────────────────────────────────────────────────

fn texto_de(v: &serde_json::Value, campo: &str, max: usize) -> String {
    let s = v.get(campo).and_then(|x| x.as_str()).unwrap_or_default();
    s.chars().take(max).collect()
}

fn bool_de(v: &serde_json::Value, campo: &str) -> bool {
    v.get(campo).and_then(|x| x.as_bool()).unwrap_or(false)
}

fn descricao_de(v: &serde_json::Value) -> Descricao {
    Descricao {
        tag: limpo(&texto_de(v, "tag", 20), 20).to_lowercase(),
        tipo: limpo(&texto_de(v, "tipo", 20), 20).to_lowercase(),
        rotulo: limpo(&texto_de(v, "rotulo", MAX_ROTULO * 2), MAX_ROTULO),
        destino: limpo(&texto_de(v, "destino", MAX_DESTINO * 2), MAX_DESTINO),
        secreto: bool_de(v, "secreto"),
        editavel: bool_de(v, "editavel"),
    }
}

/// Rebuilds a `browser.read` result from the fields we know, with the caps we set. A page can
/// make any value in its answer as large or as odd as it likes; what leaves for the server is
/// only what this function writes, so an unexpected field is dropped and not forwarded.
fn sanear_leitura(v: &serde_json::Value) -> serde_json::Value {
    let elementos: Vec<serde_json::Value> = v
        .get("elementos")
        .and_then(|x| x.as_array())
        .map(|a| a.as_slice())
        .unwrap_or_default()
        .iter()
        .take(MAX_ELEMENTOS)
        .filter_map(|e| {
            let ref_ = e.get("ref").and_then(|x| x.as_u64())?;
            let d = descricao_de(e);
            let mut o = serde_json::json!({
                "ref": ref_,
                "tag": d.tag,
                "tipo": d.tipo,
                "rotulo": d.rotulo,
                "destino": d.destino,
                "secreto": d.secreto,
            });
            if e.get("preenchido").is_some() {
                o["preenchido"] = serde_json::json!(bool_de(e, "preenchido"));
            }
            Some(o)
        })
        .collect();
    let texto = texto_de(v, "texto", MAX_TEXTO);
    serde_json::json!({
        "url": limpo(&texto_de(v, "url", 500), 500),
        "titulo": limpo(&texto_de(v, "titulo", 400), 200),
        "texto": texto,
        "textoCortado": bool_de(v, "textoCortado") || v.get("texto").and_then(|x| x.as_str()).is_some_and(|t| t.chars().count() > MAX_TEXTO),
        "elementos": elementos,
        "totalElementos": v.get("totalElementos").and_then(|x| x.as_u64()).unwrap_or(0).min(100_000),
    })
}

/// Why a page's own `{erro: …}` became the code the model sees.
fn falha_da_pagina(codigo: &str) -> Falha {
    match codigo {
        "desatualizado" => (
            "desatualizado",
            "a página mudou desde a última leitura; leia de novo (browser.read) antes de agir".to_string(),
        ),
        "mudou" => (
            "desatualizado",
            "o elemento mudou depois que a pessoa foi consultada; nada foi feito".to_string(),
        ),
        "campo_secreto" => (
            "campo_secreto",
            "esse campo guarda um segredo; só a pessoa digita nele".to_string(),
        ),
        "campo_nao_suportado" => (
            "campo_nao_suportado",
            "só dá para digitar em campos de texto comuns (não em listas, caixas ou editores ricos)".to_string(),
        ),
        _ => ("falhou", "o script na página falhou".to_string()),
    }
}

// ─── the window ───────────────────────────────────────────────────────────────────────────────

fn janela_aberta(app: &tauri::AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(JANELA)
}

/// Runs one operation of `navegador.js` in the window and waits for its answer.
fn rodar(win: &WebviewWindow, op: &str, args: &serde_json::Value) -> Result<serde_json::Value, Falha> {
    let js = format!("({JS})({}, {})", serde_json::Value::String(op.to_string()), args);
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    win.eval_with_callback(js, move |s| {
        let _ = tx.send(s);
    })
    .map_err(|e| ("falhou", format!("não consegui falar com a janela do navegador: {e}")))?;
    let bruto = rx.recv_timeout(PRAZO_SCRIPT).map_err(|_| {
        (
            "sem_resposta",
            "a página não respondeu ao script (pode estar carregando, ou a janela foi fechada)".to_string(),
        )
    })?;
    if bruto.len() > MAX_RESPOSTA {
        return Err(("grande_demais", "a página devolveu mais do que o app aceita".to_string()));
    }
    let v: serde_json::Value =
        serde_json::from_str(&bruto).map_err(|_| ("falhou", "a página devolveu algo ilegível".to_string()))?;
    if let Some(e) = v.get("erro").and_then(|x| x.as_str()) {
        return Err(falha_da_pagina(e));
    }
    Ok(v)
}

/// The profile folder: cookies and logins of the sites the person signs in to, in a folder of the
/// app only the person can read. It is not the profile of any other browser, and not the one of
/// the ShvIA window.
#[cfg(not(target_os = "macos"))]
fn pasta_do_perfil(app: &tauri::AppHandle) -> Result<std::path::PathBuf, Falha> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| ("falhou", format!("sem pasta de dados do app: {e}")))?
        .join("navegador-do-agente");
    #[cfg(unix)]
    let feito = {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir)
    };
    #[cfg(not(unix))]
    let feito = std::fs::create_dir_all(&dir);
    feito.map_err(|e| ("falhou", format!("não consegui criar a pasta do perfil: {e}")))?;
    Ok(dir)
}

fn criar_janela(app: &tauri::AppHandle, url: Url) -> Result<WebviewWindow, Falha> {
    let construtor = WebviewWindowBuilder::new(app, JANELA, WebviewUrl::External(url))
        .title("Navegador do agente — ShvIA")
        .inner_size(1100.0, 760.0)
        .min_inner_size(480.0, 360.0)
        .on_navigation(navegacao_permitida)
        // A pop-up is not opened: login flows that need one do not work here, and the page
        // never gets a second window of its own.
        .on_new_window(|_, _| NewWindowResponse::Deny)
        // Nothing is downloaded to the person's disk on a page's say-so.
        .on_download(|_, evento| !matches!(evento, DownloadEvent::Requested { .. }))
        .on_page_load(|_, carga| {
            if let PageLoadEvent::Finished = carga.event() {
                carga_terminou();
            }
        });
    #[cfg(not(target_os = "macos"))]
    let construtor = construtor.data_directory(pasta_do_perfil(app)?);
    #[cfg(target_os = "macos")]
    let construtor = construtor.data_store_identifier(ID_DO_PERFIL);
    let win = construtor
        .build()
        .map_err(|e| ("falhou", format!("não consegui abrir a janela do navegador: {e}")))?;
    win.on_window_event(|evento| {
        if let WindowEvent::Destroyed = evento {
            esquecer();
        }
    });
    Ok(win)
}

// ─── the commands ─────────────────────────────────────────────────────────────────────────────

fn expirou(inicio: Instant, teto: Duration) -> Result<(), Falha> {
    if vencido(inicio.elapsed(), teto) {
        Err(("expirou", "o pedido expirou antes de a pessoa responder — nada foi feito".to_string()))
    } else {
        Ok(())
    }
}

/// Where the window is now. The title and the address are the agent's to see only on a site the
/// person allowed; on another (a login that redirected, a link followed) it learns the site and
/// that it has to ask to read.
fn posicao(win: &WebviewWindow) -> Result<serde_json::Value, Falha> {
    let quem = rodar(win, "quem", &serde_json::json!({}))?;
    let url = Url::parse(&texto_de(&quem, "url", 2000)).ok();
    let host = url.as_ref().map(host_de).unwrap_or_default();
    if estado().concedidos.contains(&host) {
        let caminho = url.map(|u| format!("{}://{}{}", u.scheme(), host, u.path())).unwrap_or_default();
        Ok(serde_json::json!({ "host": host, "url": limpo(&caminho, 500), "titulo": limpo(&texto_de(&quem, "titulo", 400), 200) }))
    } else {
        Ok(serde_json::json!({ "host": host, "outroSite": true }))
    }
}

fn abrir(
    window: &WebviewWindow,
    motivo: &str,
    args: &serde_json::Value,
    inicio: Instant,
    teto: Duration,
) -> Result<serde_json::Value, Falha> {
    let url = url_permitida(args.get("url").and_then(|x| x.as_str()).unwrap_or_default())?;
    let host = host_de(&url);
    let ja_concedido = estado().concedidos.contains(&host);
    if !ja_concedido && !perguntar(window, texto_abrir(&url, &host, motivo, teto), "Abrir") {
        return Err(("recusado", "a pessoa recusou abrir esse endereço — nada foi aberto".to_string()));
    }
    expirou(inicio, teto)?;
    estado().concedidos.insert(host);
    let app = window.app_handle();
    let antes = estado().cargas;
    let win = match janela_aberta(app) {
        Some(w) => {
            w.navigate(url).map_err(|e| ("falhou", format!("não consegui navegar: {e}")))?;
            w
        }
        None => criar_janela(app, url)?,
    };
    let _ = win.show();
    let _ = win.unminimize();
    let carregou = esperar_carga(antes, PRAZO_CARGA);
    if carregou {
        std::thread::sleep(ASSENTAR);
    }
    let mut r = posicao(&win)?;
    r["carregou"] = serde_json::json!(carregou);
    Ok(r)
}

fn ler(
    window: &WebviewWindow,
    motivo: &str,
    inicio: Instant,
    teto: Duration,
) -> Result<serde_json::Value, Falha> {
    let win = janela_aberta(window.app_handle())
        .ok_or(("sem_janela", "não há navegador aberto; abra uma página antes (browser.open)".to_string()))?;
    let quem = rodar(&win, "quem", &serde_json::json!({}))?;
    let host = Url::parse(&texto_de(&quem, "url", 2000)).map(|u| host_de(&u)).unwrap_or_default();
    let concedido = estado().concedidos.contains(&host);
    if !concedido {
        let texto = texto_ler(&host, &texto_de(&quem, "titulo", 400), motivo, teto);
        if !perguntar(window, texto, "Permitir a leitura") {
            return Err(("recusado", "a pessoa recusou a leitura dessa página — nada foi enviado".to_string()));
        }
    }
    expirou(inicio, teto)?;
    estado().concedidos.insert(host);
    let geracao = {
        let mut e = estado();
        e.geracao += 1;
        e.geracao
    };
    let v = rodar(&win, "ler", &serde_json::json!({ "gen": geracao }))?;
    Ok(sanear_leitura(&v))
}

/// A typed text is refused before anyone is asked when it cannot be what the agent says it is.
fn checar_digitacao(desc: &Descricao, texto: &str) -> Result<(), Falha> {
    if desc.secreto {
        return Err(falha_da_pagina("campo_secreto"));
    }
    if !desc.editavel {
        return Err(falha_da_pagina("campo_nao_suportado"));
    }
    if texto.is_empty() || texto.chars().count() > MAX_DIGITADO || texto.chars().any(|c| c.is_control() && c != '\n') {
        return Err((
            "argumento_invalido",
            format!("o texto precisa ter de 1 a {MAX_DIGITADO} caracteres, sem caracteres de controle"),
        ));
    }
    Ok(())
}

fn agir(
    window: &WebviewWindow,
    acao: Acao,
    motivo: &str,
    args: &serde_json::Value,
    inicio: Instant,
    teto: Duration,
) -> Result<serde_json::Value, Falha> {
    let win = janela_aberta(window.app_handle())
        .ok_or(("sem_janela", "não há navegador aberto; abra uma página antes (browser.open)".to_string()))?;
    let ref_ = args
        .get("ref")
        .and_then(|x| x.as_u64())
        .filter(|n| (1..=1000).contains(n))
        .ok_or(("argumento_invalido", "falta o número do elemento (ref), que vem de browser.read".to_string()))?;
    let texto = match acao {
        Acao::Digitar => Some(args.get("texto").and_then(|x| x.as_str()).unwrap_or_default().to_string()),
        Acao::Clicar => None,
    };
    let geracao = estado().geracao;
    if geracao == 0 {
        return Err(("sem_leitura", "leia a página antes (browser.read): os números dos elementos vêm dela".to_string()));
    }
    let d = rodar(&win, "descrever", &serde_json::json!({ "gen": geracao, "ref": ref_ }))?;
    let desc = descricao_de(&d);
    let pagina = Url::parse(&texto_de(&d, "url", 2000)).ok();
    let host = pagina.as_ref().map(host_de).unwrap_or_default();
    if !estado().concedidos.contains(&host) {
        return Err(("desatualizado", "a janela está em outro site; leia a página de novo (browser.read)".to_string()));
    }
    if let Some(t) = &texto {
        checar_digitacao(&desc, t)?;
    }
    if !perguntar(
        window,
        texto_acao(acao, &host, &desc, texto.as_deref(), motivo, teto),
        match acao {
            Acao::Clicar => "Clicar",
            Acao::Digitar => "Digitar",
        },
    ) {
        return Err(("recusado", "a pessoa recusou — nada foi feito na página".to_string()));
    }
    expirou(inicio, teto)?;
    let antes = estado().cargas;
    rodar(
        &win,
        "agir",
        &serde_json::json!({
            "gen": geracao,
            "ref": ref_,
            "acao": if acao == Acao::Clicar { "clicar" } else { "digitar" },
            "texto": texto.unwrap_or_default(),
            "impressao": texto_de(&d, "impressao", 600),
        }),
    )?;
    let navegou = esperar_carga(antes, PRAZO_NAVEGACAO);
    if navegou {
        std::thread::sleep(ASSENTAR);
    }
    let mut r = posicao(&win)?;
    r["feito"] = serde_json::json!(true);
    r["navegou"] = serde_json::json!(navegou);
    Ok(r)
}

fn fechar(window: &WebviewWindow) -> Result<serde_json::Value, Falha> {
    let tinha = janela_aberta(window.app_handle());
    if let Some(w) = &tinha {
        let _ = w.destroy();
    }
    esquecer();
    Ok(serde_json::json!({ "fechada": tinha.is_some() }))
}

/// Runs one command of this family. Called off the UI thread: it waits on dialogs and pages.
pub(super) fn executar(
    window: &WebviewWindow,
    comando: &str,
    motivo: &str,
    args: &serde_json::Value,
    inicio: Instant,
    teto: Duration,
) -> (bool, serde_json::Value) {
    let r = match comando {
        "browser.open" => abrir(window, motivo, args, inicio, teto),
        "browser.read" => ler(window, motivo, inicio, teto),
        "browser.click" => agir(window, Acao::Clicar, motivo, args, inicio, teto),
        "browser.type" => agir(window, Acao::Digitar, motivo, args, inicio, teto),
        "browser.close" => fechar(window),
        _ => Err(("comando_desconhecido", "comando de navegador que este app não tem".to_string())),
    };
    match r {
        Ok(v) => (true, v),
        Err((codigo, msg)) => (false, falha(codigo, &msg)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn so_http_e_https_sem_credencial_sao_abertos() {
        for bom in ["https://painel.exemplo.com.br/entrar", "http://127.0.0.1:8080/x?y=1", "  https://a.com  "] {
            assert!(url_permitida(bom).is_ok(), "{bom}");
        }
        for ruim in [
            "",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "shvia://abrir?x=1",
            "ftp://servidor/arquivo",
            "data:text/html,<h1>x</h1>",
            "https://banco.com@evil.example/",
            "https://usuario:senha@exemplo.com/",
            "exemplo.com/sem-esquema",
        ] {
            assert!(url_permitida(ruim).is_err(), "{ruim} passou");
        }
        assert!(url_permitida(&format!("https://a.com/{}", "x".repeat(2100))).is_err());
        assert_eq!(url_permitida("file:///etc/passwd").unwrap_err().0, "argumento_invalido");
    }

    #[test]
    fn o_site_da_concessao_e_minusculo_e_leva_a_porta_quando_nao_e_a_padrao() {
        assert_eq!(host_de(&url("https://Painel.Exemplo.com/a")), "painel.exemplo.com");
        assert_eq!(host_de(&url("https://painel.exemplo.com:443/a")), "painel.exemplo.com");
        assert_eq!(host_de(&url("http://127.0.0.1:8765/a")), "127.0.0.1:8765");
        // Two ports of one machine are two sites for a grant.
        assert_ne!(host_de(&url("http://127.0.0.1:8765/")), host_de(&url("http://127.0.0.1:9000/")));
    }

    #[test]
    fn rede_propria_e_reconhecida_e_a_internet_nao() {
        for local in [
            "http://localhost:3000/",
            "http://127.0.0.1/",
            "http://192.168.0.1/admin",
            "http://10.1.2.3/",
            "http://172.16.0.9/",
            "http://169.254.1.1/",
            "http://[::1]/",
            "http://[fe80::1]/",
            "http://[fd12:3456::1]/",
            "http://roteador/",
            "http://impressora.local/",
            "http://app.localhost/",
        ] {
            assert!(endereco_local(&url(local)), "{local}");
        }
        for fora in ["https://exemplo.com/", "http://8.8.8.8/", "http://172.32.0.1/", "https://sub.exemplo.com.br/"] {
            assert!(!endereco_local(&url(fora)), "{fora}");
        }
    }

    #[test]
    fn navegacao_dentro_da_janela_so_para_paginas_da_web() {
        assert!(navegacao_permitida(&url("https://a.com/")));
        assert!(navegacao_permitida(&url("http://a.com/")));
        assert!(navegacao_permitida(&url("about:blank")));
        for ruim in ["file:///etc/passwd", "shvia://x", "javascript:void(0)", "data:text/html,x", "blob:https://a.com/1"] {
            assert!(!navegacao_permitida(&url(ruim)), "{ruim}");
        }
    }

    #[test]
    fn texto_citado_perde_o_que_reordena_ou_esconde() {
        // An override character can make the end of a sentence read as its beginning.
        assert_eq!(limpo("Cancelar\u{202E}ratnoc", 50), "Cancelarratnoc");
        assert_eq!(limpo("a\u{200B}b\u{2066}c\u{FEFF}d", 50), "abcd");
        assert_eq!(limpo("uma\nlinha\t só\r", 50), "uma linha só");
        assert_eq!(limpo(&"x".repeat(200), 10), format!("{}…", "x".repeat(10)));
        assert_eq!(limpo("curto", 10), "curto");
    }

    #[test]
    fn o_dialogo_de_abrir_cita_o_motivo_como_do_agente_e_avisa_da_rede_propria() {
        let u = url("https://painel.exemplo.com/entrar");
        let t = texto_abrir(&u, "painel.exemplo.com", "ver a fatura de setembro", Duration::from_secs(170));
        assert!(t.contains("nas palavras do agente (não verificado): “ver a fatura de setembro”"));
        assert!(t.contains("painel.exemplo.com"));
        assert!(t.contains("servidor do ShvIA"));
        assert!(t.contains("o agente não recebe o que você digita"));
        assert!(!t.contains("Atenção: este endereço é da sua própria rede"));

        let r = url("http://192.168.0.1/admin");
        let t = texto_abrir(&r, "192.168.0.1", "", Duration::from_secs(60));
        assert!(t.contains("Atenção: este endereço é da sua própria rede"));
        assert!(t.contains("nenhum"));
        assert!(t.contains("60 segundos"));
    }

    #[test]
    fn os_dialogos_de_clicar_e_digitar_dizem_que_a_descricao_e_da_pagina() {
        let botao = Descricao { tag: "button".into(), rotulo: "Pagar agora".into(), ..Default::default() };
        let t = texto_acao(Acao::Clicar, "loja.exemplo.com", &botao, None, "concluir", Duration::from_secs(90));
        assert!(t.contains("Elemento, como a página o descreve: botão “Pagar agora”"));
        assert!(t.contains("pode enviar um formulário, comprar, apagar ou publicar"));
        assert!(!t.contains("Texto:"));

        let link = Descricao { tag: "a".into(), rotulo: "Ver faturas".into(), destino: "exemplo.com/faturas".into(), ..Default::default() };
        assert!(link.por_extenso().contains("link “Ver faturas”, que vai para exemplo.com/faturas"));

        let campo = Descricao { tag: "input".into(), tipo: "text".into(), rotulo: "Usuário".into(), editavel: true, ..Default::default() };
        let t = texto_acao(Acao::Digitar, "painel.exemplo.com", &campo, Some("samir"), "entrar", Duration::from_secs(90));
        assert!(t.contains("Campo, como a página o descreve: campo de texto “Usuário”"));
        assert!(t.contains("Texto: “samir”"));
        assert!(t.contains("vai para o site"));
    }

    #[test]
    fn a_leitura_sai_so_com_os_campos_conhecidos_e_dentro_dos_limites() {
        let muitos: Vec<serde_json::Value> = (1..=400)
            .map(|n| serde_json::json!({ "ref": n, "tag": "a", "tipo": null, "rotulo": "x".repeat(900), "destino": "d".repeat(900), "secreto": false, "valorSecreto": "NAO", "preenchido": true }))
            .collect();
        let bruto = serde_json::json!({
            "url": "https://a.com/p",
            "titulo": "t".repeat(900),
            "texto": "palavra ".repeat(9000),
            "textoCortado": false,
            "elementos": muitos,
            "totalElementos": 400,
            "cookies": "sessao=abc",
            "inesperado": { "a": 1 },
        });
        let s = sanear_leitura(&bruto);
        assert_eq!(s["elementos"].as_array().unwrap().len(), MAX_ELEMENTOS);
        assert_eq!(s["elementos"][0]["rotulo"].as_str().unwrap().chars().count(), MAX_ROTULO + 1);
        assert!(s["elementos"][0]["destino"].as_str().unwrap().chars().count() <= MAX_DESTINO + 1);
        assert_eq!(s["texto"].as_str().unwrap().chars().count(), MAX_TEXTO);
        assert_eq!(s["textoCortado"], true, "cut by Rust even when the page says it did not");
        assert!(s["titulo"].as_str().unwrap().chars().count() <= 201);
        assert!(s.get("cookies").is_none() && s.get("inesperado").is_none());
        assert!(s["elementos"][0].get("valorSecreto").is_none());
        assert_eq!(s["elementos"][0]["preenchido"], true);
    }

    #[test]
    fn uma_leitura_malformada_vira_vazia_e_nao_quebra() {
        for ruim in [serde_json::json!(null), serde_json::json!("texto"), serde_json::json!({ "elementos": "nao" }), serde_json::json!({ "elementos": [1, "a", null, { "sem": "ref" }] })] {
            let s = sanear_leitura(&ruim);
            assert_eq!(s["elementos"].as_array().unwrap().len(), 0);
            assert_eq!(s["texto"], "");
        }
    }

    #[test]
    fn campo_de_segredo_nunca_recebe_texto_e_nem_pergunta() {
        let senha = Descricao { tag: "input".into(), tipo: "password".into(), rotulo: "Senha".into(), editavel: true, secreto: true, ..Default::default() };
        assert_eq!(checar_digitacao(&senha, "123").unwrap_err().0, "campo_secreto");
        let lista = Descricao { tag: "select".into(), ..Default::default() };
        assert_eq!(checar_digitacao(&lista, "x").unwrap_err().0, "campo_nao_suportado");
        let texto = Descricao { tag: "input".into(), tipo: "text".into(), editavel: true, ..Default::default() };
        assert!(checar_digitacao(&texto, "samir").is_ok());
        assert!(checar_digitacao(&texto, "linha 1\nlinha 2").is_ok());
        for ruim in ["", "\u{0}x", "a\u{7}b"] {
            assert_eq!(checar_digitacao(&texto, ruim).unwrap_err().0, "argumento_invalido", "{ruim:?}");
        }
        assert!(checar_digitacao(&texto, &"x".repeat(MAX_DIGITADO + 1)).is_err());
        assert!(checar_digitacao(&texto, &"x".repeat(MAX_DIGITADO)).is_ok());
    }

    #[test]
    fn o_erro_da_pagina_vira_o_codigo_que_o_modelo_entende() {
        assert_eq!(falha_da_pagina("desatualizado").0, "desatualizado");
        assert_eq!(falha_da_pagina("mudou").0, "desatualizado");
        assert_eq!(falha_da_pagina("campo_secreto").0, "campo_secreto");
        assert_eq!(falha_da_pagina("script_falhou").0, "falhou");
        assert_eq!(falha_da_pagina("qualquer coisa que a pagina invente").0, "falhou");
    }

    #[test]
    fn o_pedido_expirado_nao_age() {
        let agora = Instant::now();
        assert!(expirou(agora, Duration::from_secs(60)).is_ok());
        assert_eq!(
            expirou(agora - Duration::from_secs(61), Duration::from_secs(60)).unwrap_err().0,
            "expirou"
        );
    }

    #[test]
    fn a_espera_pela_carga_acorda_com_ela_e_desiste_no_prazo() {
        let antes = estado().cargas;
        assert!(!esperar_carga(antes, Duration::from_millis(30)), "no load, no wake-up");
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(40));
            carga_terminou();
        });
        assert!(esperar_carga(antes, Duration::from_secs(5)));
        t.join().unwrap();
    }

    #[test]
    fn esquecer_apaga_as_concessoes_e_os_numeros() {
        {
            let mut e = estado();
            e.concedidos.insert("a.com".into());
            e.geracao = 7;
        }
        esquecer();
        assert!(estado().concedidos.is_empty());
        assert_eq!(estado().geracao, 0);
    }

    // ─── rulers: the shape of the code, not its behaviour ───────────────────────────────────

    fn so_codigo(fonte: &str) -> String {
        let corte = fonte.find("#[cfg(test)]").unwrap_or(fonte.len());
        fonte[..corte]
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The agent's window must stay outside every capability: the page in it is whatever site
    /// the agent was asked to open, and a capability would let that site call Rust.
    #[test]
    fn nenhuma_capability_cobre_a_janela_do_agente() {
        let json: serde_json::Value = serde_json::from_str(include_str!("../../capabilities/default.json")).unwrap();
        for padrao in json["windows"].as_array().unwrap() {
            let p = padrao.as_str().unwrap();
            let cobre = match p.strip_suffix('*') {
                Some(prefixo) => JANELA.starts_with(prefixo),
                None => p == JANELA,
            };
            assert!(!cobre, "the capability window pattern {p:?} covers {JANELA:?}");
        }
        assert!(json.get("remote").is_none(), "a capability granted to remote URLs would reach this window");
    }

    /// The window gets no way to call back into the app: no message handler, no command, no
    /// ipc handler. Everything it says arrives through `eval_with_callback`.
    #[test]
    fn a_janela_do_agente_nao_registra_canal_da_pagina_para_o_rust() {
        let codigo = so_codigo(include_str!("navegador.rs"));
        for proibido in ["messageHandlers", "add_script_message_handler", "with_ipc_handler", "invoke_handler", "initialization_script", "on_page_load(|webview"] {
            assert!(!codigo.contains(proibido), "{proibido} would give the page a way into the app");
        }
        assert_eq!(codigo.matches("WebviewWindowBuilder::new").count(), 1);
    }

    /// The script must answer synchronously: a Promise comes back empty from `eval_with_callback`.
    #[test]
    fn o_script_da_pagina_e_sincrono() {
        let js: String = JS.lines().filter(|l| !l.trim_start().starts_with("*") && !l.trim_start().starts_with("/*")).collect::<Vec<_>>().join("\n");
        for assincrono in ["async ", "await ", "Promise", ".then(", "setTimeout", "fetch(", "XMLHttpRequest", "WebSocket", "sendBeacon", "postMessage"] {
            assert!(!js.contains(assincrono), "{assincrono} in navegador.js: the answer must be synchronous, and the script must not talk to the network");
        }
        assert!(js.contains("catch (e)"), "an exception is swallowed on Windows, so every path must answer");
    }

    /// The numbers of a page are tied to ONE read, and the script has no other way to find an
    /// element: it never takes a selector from the agent.
    #[test]
    fn o_agente_nunca_passa_seletor_para_o_script() {
        let js = JS;
        assert!(!js.contains("a.seletor") && !js.contains("a.selector") && !js.contains("eval("), "the script must not run anything the agent wrote");
        assert!(!js.contains("new Function"));
    }

    /// Order of the gates inside the dispatcher is checked in `aparelho.rs`; here, that the
    /// consent comes before the act in each function that acts on a page.
    #[test]
    fn a_pergunta_vem_antes_do_ato() {
        let codigo = so_codigo(include_str!("navegador.rs"));
        let i = codigo.find("fn agir(").expect("agir() moved");
        let corpo = &codigo[i..codigo[i..].find("\nfn fechar(").map(|f| i + f).unwrap_or(codigo.len())];
        let pergunta = corpo.find("perguntar(").expect("agir() no longer asks");
        let ato = corpo.find("\"agir\"").expect("agir() no longer acts");
        assert!(pergunta < ato, "the page was acted on before the person was asked");
        let secreto = corpo.find("checar_digitacao(").expect("agir() no longer checks the field");
        assert!(secreto < pergunta, "the person was asked before the field was checked");
    }
}
