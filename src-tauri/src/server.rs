//! Endereço do servidor ShvIA: configuração persistida, validação e **probe de
//! alcance no Rust** (item D4; ADR-019).
//!
//! Três coisas moram aqui, e a ordem importa para entender o desenho:
//!
//! 1. **O probe saiu do JavaScript.** A casca fazia `fetch(.../api/v1/health)` com
//!    `no-cors`, e isso tinha dois problemas. O primeiro é medido e está no
//!    ADR-012: o primeiro request de rede do WebKit "frio" custa **5-6 s antes de
//!    qualquer resposta**, o que forçou um timeout de 15 s e faz o app parecer
//!    travado no splash. Um `TcpStream::connect_timeout` no Rust responde em
//!    dezenas de milissegundos. O segundo é o que **destrava este item**: o `csp`
//!    do `tauri.conf.json` é ESTÁTICO, e um `connect-src` estático não pode listar
//!    uma URL que o usuário acabou de digitar. Enquanto o probe fosse JS, servidor
//!    configurável era impossível — não por decisão, por CSP.
//!
//! 2. **Alcance não é identidade.** O probe abre um TCP e fecha. Ele responde
//!    "tem alguém escutando nessa porta", exatamente como o `no-cors` respondia
//!    (resposta opaca não deixava ler nada). Não valida certificado nem confere
//!    que é um ShvIA. Isso é **escopo declarado**, não esquecimento: falar HTTPS
//!    daqui exigiria `reqwest` + rustls, e o binário ainda não paga esse custo. O
//!    D1 (auto-update) trará um cliente HTTP de verdade; a validação de identidade
//!    entra com ele. Até lá o feedback vem da própria página: URL errada abre um
//!    site errado, e isso é visível na hora.
//!
//! 3. **The configured origin becomes internal** — it receives native Code,
//!    notification, badge and version-gate capabilities. This is a trust decision:
//!    the native configuration screen explains the access before saving.
//!    See `crate::is_server_origin` and ADR-019.

use std::io::ErrorKind;
use std::net::TcpStream;
use std::sync::RwLock;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};

/// Servidor que o app abre quando nada foi configurado.
pub const DEFAULT_URL: &str = "https://ai.shvia.org";

/// Nome do arquivo dentro do diretório de config do app.
const CONFIG_FILE: &str = "server.json";

/// Teto do probe. Um `connect` que não fecha em 4 s está, para o usuário,
/// indisponível — e o auto-retry da casca tenta de novo em 5 s. É 4 e não 15
/// (o timeout do `fetch` no ADR-012) porque aquele número existia só para
/// absorver o cold-start do WebKit, que este módulo eliminou.
const PROBE_TIMEOUT_MS: u64 = 4_000;

/// The configured server's full URL. Trust always includes scheme, host and effective port,
/// including ordinary HTTPS servers. One lock keeps every consumer on the same origin.
/// `None` means only the built-in origins apply.
static CONFIGURED_URL: RwLock<Option<tauri::Url>> = RwLock::new(None);

#[cfg(test)]
pub(crate) static CONFIG_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// O que a casca precisa saber sobre o servidor atual.
#[derive(Serialize, Clone, Debug)]
pub struct ServerConfig {
    /// Origem completa (`https://host[:porta]`), sem barra final.
    pub url: String,
    /// `true` quando é o embutido — a casca usa para não oferecer "voltar ao padrão".
    pub is_default: bool,
    /// O padrão embutido, para a UI mostrar de onde se está saindo.
    pub default_url: String,
}

impl ServerConfig {
    fn from_url(url: String) -> Self {
        Self {
            is_default: url == DEFAULT_URL,
            url,
            default_url: DEFAULT_URL.to_string(),
        }
    }
}

/// The configured host, if any, for the tray's display label. Trust uses the full URL.
pub fn configured_host() -> Option<String> {
    configured_url().and_then(|u| u.host_str().map(str::to_string))
}

/// The configured server's URL, if it is not the default (see `CONFIGURED_URL`).
pub fn configured_url() -> Option<tauri::Url> {
    CONFIGURED_URL.read().ok().and_then(|g| g.clone())
}

fn set_configured_host(url: &str) {
    let completa = if url != DEFAULT_URL { tauri::Url::parse(url).ok() } else { None };
    if let Ok(mut g) = CONFIGURED_URL.write() {
        *g = completa;
    }
}

/// Costura de teste: o perímetro (`crate::is_server_origin`) precisa ser exercitado
/// sem `AppHandle` nem disco. Só existe em `cfg(test)` para não virar uma porta de
/// trocar o host configurado em runtime sem passar pela validação do `save`.
#[cfg(test)]
pub fn set_configured_host_para_teste(url: &str) {
    set_configured_host(url);
}

fn config_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join(CONFIG_FILE))
}

/// Normaliza o que o usuário digitou, ou explica em português por que não serve.
///
/// Return only the origin: paths, queries and fragments do not change the origin
/// trusted by `is_server_origin`.
pub fn normalize(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("Informe o endereço do servidor.".into());
    }

    // Sem esquema, assume https. Quem digita "meu.servidor" quer https, e exigir
    // o prefixo só rende erro em cima de um acerto de intenção.
    let com_esquema = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("https://{raw}")
    };

    let url = tauri::Url::parse(&com_esquema).map_err(|_| "Endereço inválido.".to_string())?;

    let host = url
        .host_str()
        .filter(|h| !h.is_empty())
        .ok_or_else(|| "Endereço sem host.".to_string())?;

    // Credencial embutida na URL: recusa em vez de descartar em silêncio. Quem
    // colou `https://user:senha@host` acha que a credencial vai junto, e um
    // descarte calado deixa a pessoa achando que está autenticada.
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Não use usuário:senha no endereço.".into());
    }

    let local = host == "localhost" || host == "127.0.0.1" || host == "[::1]";
    match url.scheme() {
        "https" => {}
        // `http` só em loopback, e só para desenvolvimento. Em rede, http entrega
        // a sessão e todo o tráfego em claro — e esta casca dá ao host as pontes
        // nativas, então rebaixar o transporte é rebaixar o perímetro inteiro.
        "http" if local => {}
        "http" => return Err("Use https:// — http só é aceito em localhost.".into()),
        _ => return Err("O endereço deve começar com https://".into()),
    }

    let porta = match url.port() {
        Some(p) => format!(":{p}"),
        None => String::new(),
    };

    Ok(format!("{}://{}{}", url.scheme(), host, porta))
}

/// Lê a configuração do disco. **Nunca falha:** arquivo ausente, JSON corrompido
/// ou URL que não passa na validação de hoje caem no padrão embutido.
///
/// Fail-open aqui é a diferença entre "o app abre no ShvIA" e "o app não abre".
/// Um `server.json` editado à mão com lixo dentro não pode virar tela branca.
pub fn load(app: &AppHandle) -> ServerConfig {
    let url = config_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("url").and_then(|u| u.as_str()).map(str::to_string))
        // Revalida o que veio do disco: a regra pode ter endurecido desde que
        // aquele arquivo foi escrito, e um `http://` de rede gravado por uma
        // versão antiga não deve continuar valendo.
        .and_then(|u| normalize(&u).ok())
        .unwrap_or_else(|| DEFAULT_URL.to_string());

    set_configured_host(&url);
    ServerConfig::from_url(url)
}

/// Grava a URL (já normalizada) e atualiza o perímetro na hora.
pub fn save(app: &AppHandle, raw: &str) -> Result<ServerConfig, String> {
    let url = normalize(raw)?;

    let path = config_path(app).ok_or_else(|| "Sem diretório de configuração.".to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Não deu para criar a pasta: {e}"))?;
    }

    let json = serde_json::json!({ "url": &url }).to_string();
    std::fs::write(&path, json).map_err(|e| format!("Não deu para salvar: {e}"))?;

    // Só depois de o disco aceitar. Trocar o perímetro antes deixaria o app
    // confiando num host que não sobreviveria ao próximo boot.
    set_configured_host(&url);
    Ok(ServerConfig::from_url(url))
}

/// Volta ao servidor embutido, apagando a configuração.
pub fn reset(app: &AppHandle) -> ServerConfig {
    if let Some(p) = config_path(app) {
        let _ = std::fs::remove_file(p);
    }
    set_configured_host(DEFAULT_URL);
    ServerConfig::from_url(DEFAULT_URL.to_string())
}

/// `true` se alguém aceita conexão no host/porta da URL.
///
/// Bloqueia (DNS + connect), então o chamador precisa estar fora da thread da UI.
pub fn probe(url: &str) -> bool {
    let Ok(u) = tauri::Url::parse(url) else {
        return false;
    };
    let timeout = Duration::from_millis(PROBE_TIMEOUT_MS);

    // Resolve domains while preserving typed IP literals: `host_str()` brackets an IPv6
    // address, which `(&str, u16)::to_socket_addrs` incorrectly treats as a DNS name.
    // Keep explicit ports, the scheme's known default and the existing 443 fallback.
    let Ok(addrs) = u.socket_addrs(|| Some(443)) else {
        return false;
    };

    for addr in addrs {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(_) => return true,
            // Endereço recusado/inalcançável: tenta o próximo (host com IPv6 e
            // IPv4 onde só um responde é comum atrás de VPN).
            Err(e) if matches!(e.kind(), ErrorKind::ConnectionRefused | ErrorKind::TimedOut) => {}
            Err(_) => {}
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assume_https_quando_falta_esquema() {
        assert_eq!(normalize("meu.servidor").unwrap(), "https://meu.servidor");
        assert_eq!(
            normalize("  ai.shvia.org  ").unwrap(),
            "https://ai.shvia.org"
        );
    }

    #[test]
    fn descarta_caminho_query_e_fragmento() {
        // Guardar o caminho faria a casca navegar para ele e o perímetro comparar
        // host — duas verdades para a mesma coisa.
        assert_eq!(
            normalize("https://host/chat?x=1#frag").unwrap(),
            "https://host"
        );
    }

    #[test]
    fn preserva_porta() {
        assert_eq!(normalize("https://host:8443").unwrap(), "https://host:8443");
    }

    #[test]
    fn recusa_http_em_rede_e_aceita_em_loopback() {
        // O host configurado ganha as pontes nativas; http em rede rebaixaria o
        // perímetro inteiro, não só o transporte.
        assert!(normalize("http://servidor.interno").is_err());
        assert_eq!(
            normalize("http://localhost:8000").unwrap(),
            "http://localhost:8000"
        );
        assert_eq!(
            normalize("http://127.0.0.1:8000").unwrap(),
            "http://127.0.0.1:8000"
        );
    }

    #[test]
    fn recusa_esquema_que_nao_e_http() {
        for entrada in ["ftp://host", "file:///etc/passwd", "javascript:alert(1)"] {
            assert!(normalize(entrada).is_err(), "deveria recusar {entrada}");
        }
    }

    #[test]
    fn recusa_credencial_embutida_em_vez_de_descartar_calado() {
        // Descarte silencioso deixaria a pessoa achando que está autenticada.
        assert!(normalize("https://user:senha@host").is_err());
        assert!(normalize("https://user@host").is_err());
    }

    #[test]
    fn recusa_vazio() {
        assert!(normalize("").is_err());
        assert!(normalize("   ").is_err());
    }

    #[test]
    fn host_configurado_e_none_quando_e_o_padrao() {
        let _lock = CONFIG_TEST_LOCK.lock().unwrap();
        // Duas verdades para o mesmo host é como uma delas fica velha.
        set_configured_host(DEFAULT_URL);
        assert_eq!(configured_host(), None);

        set_configured_host("https://outro.servidor");
        assert_eq!(configured_host(), Some("outro.servidor".to_string()));

        set_configured_host(DEFAULT_URL);
        assert_eq!(configured_host(), None);
    }

    #[test]
    fn probe_falha_em_host_que_nao_resolve() {
        // Sem rede não é o mesmo que travar: DNS inexistente falha na hora.
        assert!(!probe("https://host-que-nao-existe.invalid"));
        assert!(!probe("nao-e-url"));
    }

    #[test]
    fn probe_reaches_an_ipv6_literal_on_its_explicit_port() {
        let listener = std::net::TcpListener::bind((std::net::Ipv6Addr::LOCALHOST, 0))
            .expect("this test needs IPv6 loopback");
        let addr = listener.local_addr().unwrap();
        // The probe checks TCP reachability, even for an HTTPS URL; no TLS server is needed.
        for scheme in ["http", "https"] {
            let url = normalize(&format!("{scheme}://{addr}/chat")).unwrap();
            assert!(probe(&url), "the listening IPv6 server was reported offline: {url}");
        }
    }

    #[test]
    fn probe_keeps_ipv4_and_localhost_reachable() {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        for host in ["127.0.0.1", "localhost"] {
            assert!(probe(&format!("http://{host}:{port}")), "could not reach {host}");
        }
    }

    #[test]
    fn probe_refuses_a_port_without_a_listener() {
        // Port zero cannot be a listening service's assigned port.
        assert!(!probe("http://127.0.0.1:0"));
        assert!(!probe("http://[::1]:0"));
    }
}
