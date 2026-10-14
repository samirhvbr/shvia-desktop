//! Handing the token to the ShvIA server the app is configured for (1.28.0).
//!
//! The request is the one Settings → Engines already makes — `PUT /api/auth/me/claude-subscription` with the
//! person's session — made here so the token never has to be in the page. The destination is the server the
//! shell is configured for, never anything the page names; the page's only part is the access token it hands over.

use super::tela::Segredo;
use super::{falha, Falha};
use std::io::Read;
use std::time::Duration;

/// A server the token may go to: its origin, and whether anything but HTTPS may reach it.
pub(super) struct Servidor {
    pub(super) base: String,
    pub(super) https_so: bool,
}

/// The server the shell is configured for (the default one when none is).
pub(super) fn servidor_configurado() -> Result<Servidor, Falha> {
    let url = crate::server::configured_url()
        .or_else(|| tauri::Url::parse(crate::server::DEFAULT_URL).ok())
        .ok_or_else(|| falha("servidor_inseguro", "O endereço do servidor do ShvIA não é válido."))?;
    servidor_de(&url)
}

/// HTTPS, no credentials in the address; plain HTTP only on this machine, where the token goes nowhere.
pub(super) fn servidor_de(url: &tauri::Url) -> Result<Servidor, Falha> {
    let inseguro = || {
        falha(
            "servidor_inseguro",
            "Só envio o token a um servidor HTTPS (ou a este computador). Confira o endereço do servidor do ShvIA.",
        )
    };
    if !url.username().is_empty() || url.password().is_some() {
        return Err(inseguro());
    }
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    match url.scheme() {
        "https" => {}
        "http" if local => {}
        _ => return Err(inseguro()),
    }
    Ok(Servidor { base: url.origin().ascii_serialization(), https_so: url.scheme() == "https" })
}

/// The panel's access token the page hands over. Only the shape is checked here (a Sanctum token is `id|secret`);
/// the server decides whether it is good. The check keeps a header-breaking character out of the request.
pub(super) fn validar_acesso(texto: &str) -> Result<Segredo, Falha> {
    let t = texto.trim();
    let forma = (20..=200).contains(&t.len())
        && t.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '|' | '_' | '-' | '.'));
    if !forma {
        return Err(falha("sessao_invalida", "A sessão do ShvIA não está disponível. Entre de novo e tente outra vez."));
    }
    Ok(Segredo::novo(t))
}

const ROTA: &str = "/api/auth/me/claude-subscription";

/// What the server's answer says, as the page can use it. The server's own sentence is kept when it has one: it
/// is written for the person (a token that is an API key, a session that is not allowed) and holds nothing of
/// the request.
fn frase_do_servidor(corpo: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(corpo).ok()?;
    let m = v.get("message")?.as_str()?;
    let limpa: String = m.chars().filter(|c| !c.is_control()).take(300).collect();
    // Belt and braces: a sentence that carries anything token-shaped is not shown.
    (!limpa.is_empty() && !limpa.contains("sk-ant-")).then_some(limpa)
}

/// One attempt. `Ok` only for an answer that says the subscription is connected.
fn tentar(servidor: &Servidor, acesso: &Segredo, token: &Segredo) -> Result<(), Falha> {
    // The updater's TLS provider, shared: no second crypto backend (same as the history upload).
    let _ = rustls::crypto::ring::default_provider().install_default();
    let inacessivel = || falha("servidor_inacessivel", "Não consegui falar com o ShvIA. Confira a conexão e tente de novo.");
    let cliente = reqwest::blocking::Client::builder()
        .https_only(servidor.https_so)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| inacessivel())?;
    // The characters of a valid token need no JSON escaping, which is checked before this is called.
    let corpo = Segredo::novo(&format!("{{\"token\":\"{}\"}}", token.como_str()));
    let resposta = cliente
        .put(format!("{}{ROTA}", servidor.base))
        .bearer_auth(acesso.como_str())
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .body(corpo.como_str().as_bytes().to_vec())
        .send()
        .map_err(|_| inacessivel())?;
    let status = resposta.status().as_u16();
    let mut lido = Vec::new();
    let _ = resposta.take(4097).read_to_end(&mut lido);
    if lido.len() > 4096 {
        return Err(falha("resposta_inesperada", "O ShvIA respondeu algo que não esperava."));
    }
    match status {
        200 => {
            let conectado = serde_json::from_slice::<serde_json::Value>(&lido)
                .ok()
                .and_then(|v| v.get("connected").and_then(|c| c.as_bool()));
            if conectado == Some(true) {
                Ok(())
            } else {
                Err(falha("resposta_inesperada", "O ShvIA respondeu, mas não confirmou a conexão."))
            }
        }
        401 => Err(falha("sessao_invalida", "A sessão do ShvIA venceu. Entre de novo e repita a conexão.")),
        429 => Err(falha("servidor_ocupado", "O ShvIA pediu para esperar um pouco. Tente de novo em um minuto.")),
        s if s >= 500 => Err(falha("servidor_com_erro", "O ShvIA está com problema agora. Tente de novo em instantes.")),
        _ => Err(falha(
            "servidor_recusou",
            frase_do_servidor(&lido).unwrap_or_else(|| "O ShvIA não aceitou o token.".to_string()),
        )),
    }
}

/// Sends the token, trying again after a pause when the failure is the network's or the server's own trouble.
/// An answer that says no (the session, the token) is final: the same request would get the same no. The
/// pauses are a parameter so a test does not wait them.
pub(super) fn entregar(servidor: &Servidor, acesso: &Segredo, token: &Segredo, pausas: &[Duration]) -> Result<(), Falha> {
    let mut pausas = pausas.iter();
    loop {
        match tentar(servidor, acesso, token) {
            Ok(()) => return Ok(()),
            Err(f) if matches!(f.codigo, "servidor_inacessivel" | "servidor_com_erro") => match pausas.next() {
                Some(p) => std::thread::sleep(*p),
                None => return Err(f),
            },
            Err(f) => return Err(f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    const TOKEN: &str = "sk-ant-oat01-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCd";
    const ACESSO: &str = "17|AbCdEfGhIjKlMnOpQrStUvWxYz0123456789abcd";

    /// A one-shot HTTP server on this machine. It answers each connection with the next canned answer and
    /// returns the raw requests it saw, so a test can read exactly what went over the wire.
    fn servidor_falso(respostas: Vec<&'static str>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let ouvinte = TcpListener::bind("127.0.0.1:0").unwrap();
        let porta = ouvinte.local_addr().unwrap().port();
        let fio = std::thread::spawn(move || {
            let mut vistos = Vec::new();
            for resposta in respostas {
                let (mut s, _) = ouvinte.accept().unwrap();
                let mut leitor = BufReader::new(s.try_clone().unwrap());
                let mut pedido = String::new();
                let mut tamanho = 0usize;
                loop {
                    let mut linha = String::new();
                    if leitor.read_line(&mut linha).unwrap() == 0 {
                        break;
                    }
                    if let Some(v) = linha.to_ascii_lowercase().strip_prefix("content-length:") {
                        tamanho = v.trim().parse().unwrap_or(0);
                    }
                    let fim = linha == "\r\n";
                    pedido.push_str(&linha);
                    if fim {
                        break;
                    }
                }
                let mut corpo = vec![0u8; tamanho];
                leitor.read_exact(&mut corpo).unwrap();
                pedido.push_str(&String::from_utf8_lossy(&corpo));
                vistos.push(pedido);
                let _ = s.write_all(resposta.as_bytes());
            }
            vistos
        });
        (format!("http://127.0.0.1:{porta}"), fio)
    }

    fn http(status: &str, corpo: &str) -> &'static str {
        Box::leak(
            format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}",
                corpo.len()
            )
            .into_boxed_str(),
        )
    }

    fn local(base: &str) -> Servidor {
        Servidor { base: base.to_string(), https_so: false }
    }

    #[test]
    fn o_pedido_e_o_que_a_tela_de_motores_ja_faz() {
        let (base, fio) = servidor_falso(vec![http("200 OK", r#"{"connected":true,"hint":"AbCd","expired":false}"#)]);
        entregar(&local(&base), &Segredo::novo(ACESSO), &Segredo::novo(TOKEN), &[]).expect("200 com connected:true conecta");
        let vistos = fio.join().unwrap();
        assert_eq!(vistos.len(), 1);
        let pedido = &vistos[0];
        assert!(pedido.starts_with("PUT /api/auth/me/claude-subscription HTTP/1.1\r\n"), "{pedido}");
        let minusculo = pedido.to_ascii_lowercase();
        assert!(minusculo.contains(&format!("authorization: bearer {}", ACESSO.to_ascii_lowercase())), "{pedido}");
        assert!(minusculo.contains("content-type: application/json"), "{pedido}");
        assert!(minusculo.contains("accept: application/json"), "{pedido}");
        assert!(pedido.ends_with(&format!("{{\"token\":\"{TOKEN}\"}}")), "{pedido}");
    }

    #[test]
    fn cada_resposta_do_servidor_vira_o_codigo_que_a_pagina_entende() {
        let casos: Vec<(&str, &str, &str)> = vec![
            ("401 Unauthorized", r#"{"message":"Unauthenticated."}"#, "sessao_invalida"),
            ("422 Unprocessable Entity", r#"{"error":"invalid_token","message":"Cole-o inteiro."}"#, "servidor_recusou"),
            ("429 Too Many Requests", "{}", "servidor_ocupado"),
            ("403 Forbidden", "{}", "servidor_recusou"),
            ("200 OK", r#"{"connected":false}"#, "resposta_inesperada"),
            ("200 OK", "isto nao e json", "resposta_inesperada"),
            // A redirect is not followed: the token would go to wherever the answer points.
            ("302 Found", "{}", "servidor_recusou"),
        ];
        for (status, corpo, esperado) in casos {
            let (base, fio) = servidor_falso(vec![http(status, corpo)]);
            let f = entregar(&local(&base), &Segredo::novo(ACESSO), &Segredo::novo(TOKEN), &[]).unwrap_err();
            fio.join().unwrap();
            assert_eq!(f.codigo, esperado, "{status} {corpo}");
        }
        // The server's own sentence is shown when it has one.
        let (base, fio) = servidor_falso(vec![http("422 Unprocessable Entity", r#"{"message":"Cole-o inteiro."}"#)]);
        let f = entregar(&local(&base), &Segredo::novo(ACESSO), &Segredo::novo(TOKEN), &[]).unwrap_err();
        fio.join().unwrap();
        assert_eq!(f.mensagem, "Cole-o inteiro.");
    }

    /// A sentence from the server that carries something token-shaped is never shown.
    #[test]
    fn uma_frase_com_cara_de_token_nao_chega_a_tela() {
        let (base, fio) = servidor_falso(vec![http("422 Unprocessable Entity", &format!(r#"{{"message":"ruim: {TOKEN}"}}"#))]);
        let f = entregar(&local(&base), &Segredo::novo(ACESSO), &Segredo::novo(TOKEN), &[]).unwrap_err();
        fio.join().unwrap();
        assert!(!f.mensagem.contains("sk-ant"), "{}", f.mensagem);
    }

    #[test]
    fn a_rede_que_cai_tenta_de_novo_e_a_resposta_que_nega_nao() {
        // Down twice, then up: connected.
        let porta_morta = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let morto = local(&format!("http://127.0.0.1:{porta_morta}"));
        let f = entregar(&morto, &Segredo::novo(ACESSO), &Segredo::novo(TOKEN), &[Duration::from_millis(1)]).unwrap_err();
        assert_eq!(f.codigo, "servidor_inacessivel");

        let (base, fio) = servidor_falso(vec![
            http("503 Service Unavailable", "{}"),
            http("200 OK", r#"{"connected":true}"#),
        ]);
        entregar(&local(&base), &Segredo::novo(ACESSO), &Segredo::novo(TOKEN), &[Duration::from_millis(1)])
            .expect("a 503 is the server's trouble: the second attempt connects");
        assert_eq!(fio.join().unwrap().len(), 2);

        // A "no" is final: one request, not three.
        let (base, fio) = servidor_falso(vec![http("401 Unauthorized", "{}")]);
        let f = entregar(
            &local(&base),
            &Segredo::novo(ACESSO),
            &Segredo::novo(TOKEN),
            &[Duration::from_millis(1), Duration::from_millis(1)],
        )
        .unwrap_err();
        assert_eq!(f.codigo, "sessao_invalida");
        assert_eq!(fio.join().unwrap().len(), 1);
    }

    /// SOURCE check, declared as such: a redirect answer is not followed (the token would go wherever the answer
    /// points), and the HTTPS-only switch is the one `servidor_de` decided. Following a redirect is one builder call
    /// away, and the happy path does not exercise it.
    #[test]
    fn o_cliente_nao_segue_redirecionamento_e_respeita_o_https_so() {
        let fonte = include_str!("servidor.rs");
        let corpo = fonte.split("fn tentar(").nth(1).and_then(|x| x.split("\n}\n").next()).unwrap_or("");
        assert!(!corpo.is_empty(), "a função sumiu — esta régua mediria o nada");
        assert!(corpo.contains(".redirect(reqwest::redirect::Policy::none())"), "redirecionamento seguido");
        assert!(corpo.contains(".https_only(servidor.https_so)"), "o https_so decidido em servidor_de deixou de valer");
    }

    #[test]
    fn so_https_ou_este_computador() {
        let ok = |s: &str| servidor_de(&tauri::Url::parse(s).unwrap());
        assert_eq!(ok("https://ai.shvia.org").unwrap().base, "https://ai.shvia.org");
        assert!(ok("https://ai.shvia.org").unwrap().https_so);
        assert_eq!(ok("https://ai.shvia.org/qualquer/caminho?x=1").unwrap().base, "https://ai.shvia.org");
        assert_eq!(ok("https://dev.shvia.org:8443").unwrap().base, "https://dev.shvia.org:8443");
        let local = ok("http://127.0.0.1:8000").unwrap();
        assert_eq!((local.base.as_str(), local.https_so), ("http://127.0.0.1:8000", false));
        assert!(ok("http://localhost:8000").is_ok());
        for ruim in ["http://ai.shvia.org", "http://10.0.0.5:8000", "https://user:pw@ai.shvia.org", "ftp://ai.shvia.org"] {
            assert_eq!(ok(ruim).err().map(|f| f.codigo), Some("servidor_inseguro"), "{ruim}");
        }
    }

    #[test]
    fn o_acesso_so_passa_se_tem_a_forma_de_um() {
        assert!(validar_acesso(&format!("  {ACESSO}\n")).is_ok());
        for ruim in ["", "curto", "tem espaço no meio 1234567890123", "com\r\nquebra-1234567890123456", &"a".repeat(201)] {
            assert_eq!(validar_acesso(ruim).err().map(|f| f.codigo), Some("sessao_invalida"), "{ruim:?}");
        }
    }
}
