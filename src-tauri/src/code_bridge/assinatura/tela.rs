//! Reading the screen `claude setup-token` draws: the token it prints and the URL it shows (1.28.0).
//!
//! Everything here is pure, so the cases that matter run as fixtures without a process: a screen with
//! colour and cursor codes, a token the terminal wrapped in two, a token that is still arriving, the
//! same frame drawn twice, an API key where a subscription token should be.

use std::fmt;

/// A secret that cannot be printed by accident: `Debug` shows a mark, there is no `Display` and no
/// `Clone`, and the bytes are overwritten when it is dropped.
///
/// The overwrite is best effort and says so: a copy handed to another library (the HTTP client keeps
/// its own copy of the body) is not ours to clear. What this type does hold is the rule that matters
/// more: a `{:?}` in a panic message or a log line cannot leak the token, because the type refuses.
pub(super) struct Segredo(Vec<u8>);

impl Segredo {
    pub(super) fn novo(texto: &str) -> Self {
        Self(texto.as_bytes().to_vec())
    }

    pub(super) fn como_str(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("")
    }

    pub(super) fn zerar(&mut self) {
        for b in self.0.iter_mut() {
            // Volatile, so the optimiser cannot decide the write is dead because nobody reads it after.
            // SAFETY: `b` is a valid, aligned, exclusive reference into the vector.
            unsafe { std::ptr::write_volatile(b, 0) };
        }
    }
}

impl Drop for Segredo {
    fn drop(&mut self) {
        self.zerar();
    }
}

impl fmt::Debug for Segredo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Segredo(…)")
    }
}

/// The text of a terminal's raw output: escape sequences gone, carriage returns gone, line breaks kept.
///
/// What is removed: CSI (`ESC [` … a final byte, which is how colour, cursor movement and erasing are
/// written), OSC (`ESC ]` … BEL or `ESC \`, which is how a hyperlink is written: the client shows the URL
/// with one), the string sequences (DCS, SOS, PM, APC), charset designators (`ESC ( B`) and the two-byte
/// escapes. Cursor movement is dropped rather than followed: the client draws a frame again and again, so
/// the same text appears several times in the stream, and the readers below take the last one.
pub(super) fn limpar_ansi(bruto: &[u8]) -> String {
    let texto = String::from_utf8_lossy(bruto);
    let mut saida = String::with_capacity(texto.len());
    let mut it = texto.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\u{1b}' => match it.peek().copied() {
                Some('[') => {
                    it.next();
                    // Parameters 0x30-0x3F and intermediates 0x20-0x2F run until the final byte 0x40-0x7E.
                    for n in it.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&n) {
                            break;
                        }
                    }
                }
                Some(']') | Some('P') | Some('X') | Some('^') | Some('_') => {
                    it.next();
                    // Strings end at BEL or at ST (`ESC \`).
                    while let Some(n) = it.next() {
                        if n == '\u{7}' {
                            break;
                        }
                        if n == '\u{1b}' {
                            if it.peek() == Some(&'\\') {
                                it.next();
                            }
                            break;
                        }
                    }
                }
                Some('(') | Some(')') | Some('*') | Some('+') | Some('#') | Some('%') => {
                    it.next();
                    it.next();
                }
                Some(_) => {
                    it.next();
                }
                None => {}
            },
            '\r' => {}
            '\n' => saida.push('\n'),
            '\t' => saida.push(' '),
            c if c.is_control() => {}
            c => saida.push(c),
        }
    }
    saida
}

const PREFIXO: &str = "sk-ant-oat";

fn caractere_de_token(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// The server's own rule for what it will keep (`ClaudeSubscriptionToken::FORMAT` in ShvIA Web):
/// `sk-ant-oat`, two digits, a dash, then 20 to 500 of `[A-Za-z0-9_-]`. The same rule on both sides, so
/// nothing is sent that the server would turn away, and nothing the server would keep is turned away here.
pub(super) fn token_valido(s: &str) -> bool {
    let Some(resto) = s.strip_prefix(PREFIXO) else {
        return false;
    };
    let b = resto.as_bytes();
    if b.len() < 3 || !b[0].is_ascii_digit() || !b[1].is_ascii_digit() || b[2] != b'-' {
        return false;
    }
    let corpo = &resto[3..];
    (20..=500).contains(&corpo.len()) && corpo.chars().all(caractere_de_token)
}

const ROTULO_DO_TOKEN: &str = "Your OAuth token";
const ROTULO_DE_FIM: &str = "Store this token securely";

/// The token, from the client's screen text (already through [`limpar_ansi`]).
///
/// `terminou` says the client has exited and everything it wrote has been read: only then may a token
/// that ends the text be taken as whole. While the client still runs, a run that reaches the end of the
/// text may be half of a token that is still arriving, and sending half of one would store a token that
/// passes the server's format check and fails at the first session.
///
/// Two readings, the first preferred:
///
/// 1. **Between the client's own labels** — the line "Your OAuth token (valid for 1 year):" and the line
///    "Store this token securely…" — with every blank removed. A terminal that wraps a long line puts a
///    line break inside the token; the labels are what let it be put back together.
/// 2. **A run anywhere** that starts with `sk-ant-oat`, for a client whose wording changed. It refuses a
///    run that a wrap may have cut: one followed directly, with no blank line between, by a line made only
///    of token characters. Failing is cheap here (the person pastes the token by hand); storing a
///    truncated token is not.
pub(super) fn achar_token(tela: &str, terminou: bool) -> Option<Segredo> {
    if let Some(t) = entre_os_rotulos(tela) {
        return Some(t);
    }
    let mut achado: Option<&str> = None;
    for (i, _) in tela.match_indices(PREFIXO) {
        let resto = &tela[i..];
        let tamanho = resto.find(|c: char| !caractere_de_token(c)).unwrap_or(resto.len());
        let corrida = &resto[..tamanho];
        let depois = &resto[tamanho..];
        if depois.is_empty() && !terminou {
            continue; // may still be arriving
        }
        if cortada_por_quebra(depois) {
            continue;
        }
        if token_valido(corrida) {
            achado = Some(corrida);
        }
    }
    achado.map(Segredo::novo)
}

fn entre_os_rotulos(tela: &str) -> Option<Segredo> {
    let ini = tela.rfind(ROTULO_DO_TOKEN)?;
    let fim = tela[ini..].find(ROTULO_DE_FIM)? + ini;
    // The token starts on the line after the label.
    let comeco = tela[ini..fim].find('\n')? + ini;
    let regiao: String = tela[comeco..fim].chars().filter(|c| !c.is_whitespace()).collect();
    token_valido(&regiao).then(|| Segredo::novo(&regiao))
}

/// Is `depois` — the text right after a run of token characters — the start of its continuation on the next
/// line? True when a line break is followed at once by a line of nothing but token characters.
fn cortada_por_quebra(depois: &str) -> bool {
    let Some(resto) = depois.strip_prefix('\n') else {
        return false;
    };
    let linha = resto.split('\n').next().unwrap_or("");
    !linha.is_empty() && linha.chars().all(caractere_de_token)
}

/// The sign-in URL the client shows for a browser that did not open, if it is one the person can trust.
///
/// Only a host of Claude's own (or Anthropic's console) that looks like an authorization page qualifies.
/// The page shows the URL as a link, so a URL from the screen of a program is never offered as is.
pub(super) fn achar_url(tela: &str) -> Option<String> {
    for (i, _) in tela.match_indices("https://") {
        let bruta: String = tela[i..].chars().take_while(|c| !c.is_whitespace()).collect();
        let url = bruta.trim_end_matches(['.', ',', ')', ';', '"', '\'', '>']);
        if host_de_confianca(url) && (url.contains("oauth") || url.contains("authorize")) {
            return Some(url.to_string());
        }
    }
    None
}

fn host_de_confianca(url: &str) -> bool {
    let Some(resto) = url.strip_prefix("https://") else {
        return false;
    };
    let host = resto.split(['/', '?', '#']).next().unwrap_or("");
    // A port or credentials in the authority are not what an authorization page carries.
    if host.is_empty() || host.contains(['@', ':']) {
        return false;
    }
    ["claude.com", "claude.ai", "console.anthropic.com"]
        .iter()
        .any(|ok| host == *ok || host.ends_with(&format!(".{ok}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "sk-ant-oat01-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCd";

    /// What the client draws on success, with the colour codes of a terminal and the CRLF a terminal adds.
    fn quadro_de_sucesso(token: &str) -> String {
        format!(
            "\u{1b}[2K\u{1b}[1A\u{1b}[2K\u{1b}[G\u{1b}[32m✓ Long-lived authentication token created successfully!\u{1b}[39m\r\n\r\n\
             Your OAuth token (valid for 1 year):\r\n\r\n\u{1b}[33m{token}\u{1b}[39m\r\n\r\n\
             \u{1b}[2mStore this token securely. You won't be able to see it again.\u{1b}[22m\r\n\
             \u{1b}[2mUse this token by setting: export CLAUDE_CODE_OAUTH_TOKEN=<token>\u{1b}[22m\r\n"
        )
    }

    #[test]
    fn o_segredo_nao_se_imprime_e_se_apaga() {
        let mut s = Segredo::novo(TOKEN);
        assert_eq!(format!("{s:?}"), "Segredo(…)", "Debug não pode mostrar o valor");
        assert_eq!(s.como_str(), TOKEN);
        s.zerar();
        assert!(s.0.iter().all(|&b| b == 0), "os bytes foram apagados");
    }

    fn token_de(t: Option<Segredo>) -> Option<String> {
        t.map(|s| s.como_str().to_string())
    }

    #[test]
    fn o_ansi_sai_e_o_texto_fica() {
        let bruto = quadro_de_sucesso(TOKEN);
        let limpo = limpar_ansi(bruto.as_bytes());
        assert!(limpo.contains("✓ Long-lived authentication token created successfully!"), "{limpo}");
        assert!(limpo.contains(TOKEN), "{limpo}");
        assert!(!limpo.contains('\u{1b}') && !limpo.contains('\r'), "{limpo:?}");
        // OSC 8 hyperlink: the visible text stays, the wrapper goes (ST and BEL terminators).
        let link = "\u{1b}]8;;https://claude.com/x\u{1b}\\texto\u{1b}]8;;\u{1b}\\ e \u{1b}]0;titulo\u{7}fim";
        assert_eq!(limpar_ansi(link.as_bytes()), "texto e fim");
        // Charset designators and two-byte escapes.
        assert_eq!(limpar_ansi(b"\x1b(Bola\x1b7\x1b8!"), "ola!");
    }

    #[test]
    fn o_token_sai_do_quadro_de_sucesso() {
        let tela = limpar_ansi(quadro_de_sucesso(TOKEN).as_bytes());
        assert_eq!(token_de(achar_token(&tela, false)).as_deref(), Some(TOKEN));
    }

    /// The terminal wrapped the token in two. The labels put it back; the first line alone would have passed
    /// the server's format check and failed at the first session.
    #[test]
    fn um_token_quebrado_em_duas_linhas_volta_inteiro() {
        let (a, b) = TOKEN.split_at(60);
        let tela = limpar_ansi(quadro_de_sucesso(&format!("{a}\r\n{b}")).as_bytes());
        assert_eq!(token_de(achar_token(&tela, false)).as_deref(), Some(TOKEN));
    }

    /// Without the labels (wording changed), a run followed straight by a line of token characters may be
    /// a wrap: nothing is returned, never half a token.
    #[test]
    fn sem_rotulos_a_corrida_que_uma_quebra_pode_ter_cortado_nao_e_aceita() {
        let (a, b) = TOKEN.split_at(60);
        let tela = format!("here it is\n{a}\n{b}\n\nbye\n");
        assert!(achar_token(&tela, true).is_none());
        // The whole token on one line, labels gone: found.
        let inteira = format!("here it is\n{TOKEN}\n\nbye\n");
        assert_eq!(token_de(achar_token(&inteira, true)).as_deref(), Some(TOKEN));
    }

    /// A token that reaches the end of the text may still be arriving: not taken until the client has gone.
    #[test]
    fn a_ponta_do_texto_so_vale_depois_que_o_cliente_saiu() {
        let metade = &TOKEN[..50];
        let tela = format!("Your OAuth token (valid for 1 year):\n\n{metade}");
        assert!(achar_token(&tela, false).is_none(), "half a token, client still running");
        // Whole, but at the end of the text and the client running: still waiting for a terminator.
        let fim_do_texto = format!("blah\n{TOKEN}");
        assert!(achar_token(&fim_do_texto, false).is_none());
        assert_eq!(token_de(achar_token(&fim_do_texto, true)).as_deref(), Some(TOKEN));
    }

    /// The client redraws its frame: the same token appears twice, and the last frame is the one read.
    #[test]
    fn o_mesmo_quadro_desenhado_duas_vezes_da_um_token() {
        let um = quadro_de_sucesso(TOKEN);
        let tela = limpar_ansi(format!("{um}{um}").as_bytes());
        assert_eq!(token_de(achar_token(&tela, false)).as_deref(), Some(TOKEN));
    }

    #[test]
    fn so_o_token_de_assinatura_vale_chave_de_api_e_exemplo_nao() {
        let chave = "sk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789";
        assert!(achar_token(&format!("sua chave:\n{chave}\n\nfim\n"), true).is_none());
        // The hint line names the variable, not a token.
        assert!(achar_token("Use this token by setting: export CLAUDE_CODE_OAUTH_TOKEN=<token>\n", true).is_none());
        // Too short for the server to keep it.
        assert!(achar_token("sk-ant-oat01-curto\n\nx\n", true).is_none());
        assert!(!token_valido("sk-ant-oat1-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789"));
        assert!(!token_valido(&format!("{TOKEN} ")));
        assert!(token_valido(TOKEN));
    }

    #[test]
    fn a_url_vem_do_hyperlink_e_so_de_host_de_confianca() {
        let tela = limpar_ansi(
            "Browser didn't open? Use the url below to sign in \u{1b}]8;;https://claude.com/cai/oauth/authorize?code=true&state=QZiX\u{1b}\\https://claude.com/cai/oauth/authorize?code=true&state=QZiX\u{1b}]8;;\u{1b}\\\r\n".as_bytes(),
        );
        assert_eq!(achar_url(&tela).as_deref(), Some("https://claude.com/cai/oauth/authorize?code=true&state=QZiX"));
        // A look-alike host, a port, credentials, plain http and a docs link are not the sign-in page.
        for ruim in [
            "https://claude.com.evil.example/oauth/authorize?x=1",
            "https://evil.example/claude.com/oauth/authorize",
            "https://claude.com:444/oauth/authorize",
            "https://user@claude.com/oauth/authorize",
            "http://claude.com/oauth/authorize",
            "https://docs.claude.com/erros",
        ] {
            assert_eq!(achar_url(ruim), None, "{ruim}");
        }
        // A sub-host of Claude's own is the sign-in page too, and the full stop that ends the sentence is not
        // part of the URL.
        assert_eq!(
            achar_url("see https://platform.claude.com/oauth/authorize?x=1.").as_deref(),
            Some("https://platform.claude.com/oauth/authorize?x=1"),
        );
    }
}
