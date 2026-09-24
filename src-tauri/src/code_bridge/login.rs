//! The Claude Code login, driven from the screen (gate 2, 1.5.17 → 1.6.0): `claude auth status`
//! for the verdict, and `claude auth login` kept alive between its two steps (the URL, then the
//! pasted code), one login at a time. Since 1.6.38 a native dialog asks before it starts.
//! Split out of `code_bridge.rs` in 1.6.41.

use super::*;

/// Localiza um binário de motor (`anna` ou `claude-runner`), cross-platform.
///
/// Ordem: (1) **ao lado do executável do ShvIA Desktop**; (2) no PATH; (3) locais
/// conhecidos por SO (`~/.local/bin/<base>` no Unix, `%LOCALAPPDATA%\Programs\
/// <base>\<base>.exe` no Windows).
///
/// **O empacotado vence o do PATH, e isso é decisão** (item D5). Desde a 0.18.0 o
/// `anna` viaja no instalador como `externalBin`, e ele é o que foi testado com
/// ESTA versão do app. Um `anna` velho esquecido no PATH — o caso comum de quem
/// instalou à mão meses atrás — passaria a decidir o comportamento do Modo Code,
/// e o sintoma seria "funciona na sua máquina" sem ninguém suspeitar do PATH.
///
/// Quem quer usar o próprio `anna` de propósito ainda consegue: basta não haver
/// binário empacotado (build sem `--anna`), ou apontar o do PATH por instalação
/// separada e usar um build sem o sidecar.
///
/// **Isso vale só para o `anna`.** O `claude-runner` NÃO é empacotado — não está no
/// `externalBin` nem no `build-local.sh` —, então para ele o passo (1) nunca acerta:
/// quem responde é sempre (2)/(3), o wrapper que o `claude-runner/install.sh` deixa em
/// `~/.local/bin`. Ausência dele é instalação opcional que faltou, NUNCA regressão de
/// empacotamento — e é essa leitura errada que custou o diagnóstico de 08/09, por isso
/// os dois pontos que devolvem a ausência dizem `install.sh` na mesma frase
/// (`ERRO_RUNNER_AUSENTE`).
/// Onde a fonte do runner mora dentro do app instalado.
///
/// Separada do `instalar_claude_runner` para ter teste: o caminho é a parte que quebra em
/// silêncio quando o `bundle.resources` muda de forma, e um erro aqui só apareceria para
/// quem clica o botão numa máquina sem o repositório — a pessoa com menos condição de
/// diagnosticar.
/// Login do Claude Code conduzido pela tela, em vez de pelo terminal (portão 2).
///
/// ## O que foi MEDIDO, e por que o desenho é este
///
/// Duas medições em 16–17/09/2026, sem TTY, contra o cliente 2.1.273.
///
/// **1. O fluxo tem DOIS finais, e o segundo não passa pela tela.**
///
/// ```text
/// [0.2s out] Opening browser to sign in…
/// [0.2s out] If the browser didn't open, visit: https://claude.com/cai/oauth/authorize?…
/// [0.2s out] Paste code here if prompted >
/// [0.3s err] Invalid code. Please make sure the full code was copied.   ← código-lixo
/// [25.4s out] Login successful.                                          ← o NAVEGADOR terminou
/// exit code 0
/// ```
///
/// O CLI abre o navegador e **também** aceita um código colado. Se a pessoa autorizar na
/// aba, o processo conclui sozinho e **ninguém colou nada**. A primeira versão disto
/// esperava o código para só então aguardar o filho — e nesse final ela esperaria para
/// sempre.
///
/// **2. Código inválido NÃO encerra o processo.** `Invalid code` sai no stderr e ele
/// continua esperando. Um `wait_with_output` depois de escrever bloqueia aqui.
///
/// **3. `exit code 0` nos DOIS finais.** Ele não distingue nada, então não decide.
///
/// **4. O código NÃO é ecoado** em stdout nem stderr. Medido com código-lixo distinto.
/// Mesmo assim a saída do login **não vai para a tela** — ver `drenar_e_esperar`.
///
/// ## O desenho que isso obriga
///
/// `stdin` sai na criação. Uma thread **drena** stdout/stderr e espera o filho; quando ele
/// sai, ela avisa a página por evento. Concluir apenas **escreve** e volta na hora — quem
/// decide se deu certo é o `claude auth status`, nunca o stdout, nunca o código de saída.
pub(super) static LOGIN_EM_CURSO: OnceLock<Mutex<Option<LoginEmCurso>>> = OnceLock::new();

/// Teto de vida do processo de login.
///
/// 🔴 **Isto é ESCOLHA, não derivação, e o registro importa.** A medição que buscava o
/// timeout do próprio CLI não produziu número: ele teve **sucesso** em 25,4 s em vez de
/// desistir, porque o navegador completou o fluxo. Descobrir o timeout dele exige uma
/// execução que ninguém autoriza, e isso é gesto do dono.
///
/// 15 min é largo o bastante para uma autorização com troca de navegador, troca de conta e
/// 2FA, e curto o bastante para não deixar processo pendurado uma tarde inteira. Quando o
/// número do CLI for medido, este vira derivado e o comentário morre.
pub(super) const TETO_DO_LOGIN: std::time::Duration = std::time::Duration::from_secs(15 * 60);

pub(super) struct LoginEmCurso {
    stdin: ChildStdin,
    /// Só para matar. A espera vive na thread que drena.
    filho: std::sync::Arc<Mutex<Child>>,
    /// Which login this is (1.6.26). The waiter of an OLD login cleared the slot without
    /// looking whose it was: restart a login quickly enough and the new one was dropped —
    /// its stdin closed, the pasted code answered `sem_login`, and `cancelar_login` at exit
    /// could no longer reach its process.
    geracao: u64,
}

/// Source of `LoginEmCurso::geracao`.
pub(super) static PROXIMO_LOGIN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Clears the slot only if it still holds THIS login (1.6.26).
pub(super) fn liberar_slot_do_login(geracao: u64) {
    if let Ok(mut g) = login_slot().lock() {
        if g.as_ref().is_some_and(|l| l.geracao == geracao) {
            *g = None;
        }
    }
}

pub(super) fn login_slot() -> &'static Mutex<Option<LoginEmCurso>> {
    LOGIN_EM_CURSO.get_or_init(|| Mutex::new(None))
}

/// Cancela um login pendente, se houver. Idempotente, e chamada também no `RunEvent::Exit`.
pub fn cancelar_login() {
    if let Ok(mut g) = login_slot().lock() {
        if let Some(l) = g.take() {
            if let Ok(mut f) = l.filho.lock() {
                let _ = f.kill();
            }
        }
    }
}

/// `claude auth status --json` com o perfil de conta aplicado.
///
/// 🔴 **Isto substitui a sonda de existência no Keychain** que a proposta de contas tinha
/// desenhado, e a medição que decidiu está registrada: o slot do `claude-b3` EXISTE e este
/// comando responde `loggedIn: false`. Existir não é estar logado — credencial vencida
/// ocupa o slot igual.
///
/// 🔴 **A PROVENIÊNCIA da leitura é problema aberto, e a tela tem de tratar como tal.**
/// `configDirectory` só acompanha o `CLAUDE_CONFIG_DIR`; com `CLAUDE_SECURESTORAGE_CONFIG_DIR`
/// ele devolve a casa padrão **por desenho**, porque essa variável move só a chave da
/// credencial. Então para perfil de credencial **não há invariante a priori** de que a
/// resposta veio do perfil pedido, e o honesto é o terceiro estado.
///
/// O que ESTÁ medido: a variável não é ignorada. O mesmo comando lê `true` sob o perfil
/// `pessoal` e `false` sem ele, e o login gravou no slot do perfil sem tocar no padrão.
pub(super) fn claude_auth_status(conta: Option<&crate::contas_claude::Alvo>) -> serde_json::Value {
    let Some(bin) = resolve_bin("claude") else {
        return serde_json::json!({ "erro": "claude não encontrado no PATH", "codigo": "cli_ausente" });
    };
    let mut cmd = Command::new(bin);
    cmd.arg("auth").arg("status").arg("--json");
    if let Some(p) = crate::user_env::sidecar_path() {
        cmd.env("PATH", p);
    }
    crate::contas_claude::aplicar(&mut cmd, conta);
    match saida_com_prazo(cmd, PRAZO_AUTH_STATUS) {
        Ok(o) => {
            let txt = String::from_utf8_lossy(&o.stdout);
            match serde_json::from_str::<serde_json::Value>(txt.trim()) {
                // 🔴 `loggedIn` tem de ser BOOLEANO. Ausente ou de outro tipo é "não sei",
                // não "não logado": um JSON que mudou de forma não pode virar convite a
                // refazer um login que já está de pé.
                Ok(mut v) if v.get("loggedIn").is_some_and(|x| x.is_boolean()) => {
                    if let Some(o) = v.as_object_mut() {
                        let (p, motivo) = proveniencia(conta, o.get("configDirectory"));
                        o.insert("proveniencia".into(), serde_json::json!(p));
                        if let Some(m) = motivo {
                            o.insert("proveniencia_motivo".into(), serde_json::json!(m));
                        }
                    }
                    v
                }
                Ok(v) => serde_json::json!({
                    "erro": format!("resposta sem `loggedIn` booleano: {v}"),
                    "codigo": "resposta_sem_forma",
                }),
                Err(_) => serde_json::json!({
                    "erro": format!("resposta ilegível do `claude auth status`: {}", txt.trim()),
                    "codigo": "resposta_ilegivel",
                }),
            }
        }
        Err(e) => serde_json::json!({
            "erro": format!("não consegui perguntar ao cliente: {e}"),
            "codigo": "cli_falhou",
        }),
    }
}

/// A leitura veio do perfil que foi pedido? **É a invariante, e ela mora AQUI.**
///
/// A página nunca recebe o diretório de um perfil — por desenho, desde a ADR-033 —, então
/// ela não tem como aplicar a invariante sozinha. Se aplicasse, precisaria do caminho, e aí
/// a tela e o `spawn` seriam duas fontes sobre qual conta está valendo.
///
/// Três respostas, e a do meio é a que evita o pior erro:
///
/// - **sem perfil** → `confirmada`. Nada foi sobrescrito, então a leitura e o turno usam o
///   mesmo ambiente. A invariante existe para pegar "setei a variável e a resposta ignorou",
///   que não pode acontecer quando não se seta nada.
/// - **`CLAUDE_CONFIG_DIR`** → compara o `configDirectory` devolvido com o do perfil,
///   canonicalizado. Igual, vale o `loggedIn`; diferente ou ausente, não vale.
/// - **`CLAUDE_SECURESTORAGE_CONFIG_DIR`** → `nao_confirmavel`, **e isso não é defeito do
///   cliente**: essa variável move só a chave da credencial, então o `configDirectory` volta
///   sendo a casa padrão porque é mesmo a casa padrão. Não existe invariante a priori.
///
/// 🔴 A tela tem de renderizar `nao_confirmavel` como **"não consegui perguntar"**, nunca
/// como "não conectado". Dizer "não conectado" para uma conta de pé convida a refazer um
/// login que não precisava — que é o erro que o terceiro estado existe para evitar.
pub(super) fn proveniencia(
    conta: Option<&crate::contas_claude::Alvo>,
    dir_lido: Option<&serde_json::Value>,
) -> (&'static str, Option<String>) {
    use crate::contas_claude::Var;
    let Some(alvo) = conta else {
        return ("confirmada", None);
    };
    match alvo.var {
        Var::SecureStorage => (
            "nao_confirmavel",
            Some(
                "perfil de credencial: a variável move só a chave, então o `configDirectory` \
                 é a casa padrão por desenho e não prova de onde a resposta veio"
                    .to_string(),
            ),
        ),
        Var::ConfigDir => {
            let Some(lido) = dir_lido.and_then(|x| x.as_str()) else {
                return (
                    "nao_confirmavel",
                    Some("a resposta não trouxe `configDirectory` (cliente anterior à 2.1.268?)".to_string()),
                );
            };
            let canon = |p: &str| {
                std::fs::canonicalize(p)
                    .map(|x| x.to_string_lossy().to_string())
                    .unwrap_or_else(|_| p.trim_end_matches('/').to_string())
            };
            if canon(lido) == canon(&alvo.dir.to_string_lossy()) {
                ("confirmada", None)
            } else {
                (
                    "nao_confirmavel",
                    Some(format!("pedi {} e a resposta veio de {}", alvo.dir.display(), lido)),
                )
            }
        }
    }
}

/// Drena as duas saídas, espera o filho e avisa a página. **Uma thread por login.**
///
/// 🔴 **Drenar é obrigatório, e não é zelo.** Ler stdout só até a URL e largar o cano deixa
/// o buffer do sistema encher enquanto a pessoa está no navegador — o CLI trava no write, e
/// o login nunca termina. Foi por isso que a leitura da URL para na primeira linha que a
/// tem e o resto vai para o ralo aqui.
///
/// 🔴 **E o que é drenado NÃO vai para a tela.** A medição diz que o cliente não ecoa o
/// código, mas isso é comportamento não documentado de uma versão: mandar stdout de login
/// para a tela é uma decisão que só precisa estar errada uma vez. O evento leva o código de
/// saída e nada mais; quem diz se deu certo é o `auth status` depois.
pub(super) fn drenar_e_esperar(
    window: &WebviewWindow,
    filho: std::sync::Arc<Mutex<Child>>,
    saida: std::process::ChildStdout,
    erro: std::process::ChildStderr,
    geracao: u64,
) {
    // Os dois canos, para nenhum encher. O conteúdo é descartado de propósito.
    for cano in [
        Box::new(saida) as Box<dyn std::io::Read + Send>,
        Box::new(erro) as Box<dyn std::io::Read + Send>,
    ] {
        std::thread::spawn(move || {
            let mut leitor = BufReader::new(cano);
            let mut lixo = String::new();
            while let Ok(n) = leitor.read_line(&mut lixo) {
                if n == 0 {
                    break;
                }
                lixo.clear();
            }
        });
    }

    let win = window.clone();
    std::thread::spawn(move || {
        let inicio = std::time::Instant::now();
        let codigo = loop {
            let tentativa = filho.lock().ok().and_then(|mut f| f.try_wait().ok().flatten());
            if let Some(st) = tentativa {
                break st.code();
            }
            if inicio.elapsed() >= TETO_DO_LOGIN {
                if let Ok(mut f) = filho.lock() {
                    let _ = f.kill();
                }
                break None;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        };
        // Some do registro: quem saiu não é mais cancelável, e deixar o `Some` aqui faria
        // o próximo `iniciar_login` matar um processo que já morreu. Only if the slot is
        // still OURS: a newer login may already be there (1.6.26).
        liberar_slot_do_login(geracao);
        // `login` lets the page tell this end from a newer login's (the reply of
        // `claudeAuthLoginStart` carries the same number).
        let js = format!(
            "window.__shviaCode&&window.__shviaCode._emit({{type:'claude_login_fim',code:{},login:{}}})",
            codigo.map_or("null".to_string(), |c| c.to_string()),
            geracao,
        );
        let app = win.app_handle().clone();
        let w = win.clone();
        let _ = app.run_on_main_thread(move || {
            let _ = w.eval(&js);
        });
    });
}

/// Acha a URL de autorização na saída do `claude auth login`.
///
/// 🔴 **Isto é saída NÃO DOCUMENTADA, e cai na mesma regra que derrubou a sonda do
/// Keychain:** pode mudar sem aviso, então não pode ser fonte única de nada e precisa de
/// saída digna quando não reconhecer. As três condições estão aqui — extração tolerante,
/// fixture da versão medida, e fallback que manda a pessoa para o terminal.
///
/// **Tolerante:** pega o primeiro `https://` que pareça de autorização, em qualquer posição
/// da linha, sem depender do texto ao redor. A frase em volta já mudou entre versões — o
/// escopo do sign-in mudou na 2.1.273 — e casar a frase seria escolher o pedaço frágil.
///
/// **Lê BYTE A BYTE de propósito.** Um `BufReader` leria adiante e engoliria bytes que a
/// thread de drenagem precisa; o que sobra no buffer dele não volta para o cano.
pub(super) fn ler_url(saida: &mut std::process::ChildStdout) -> Option<String> {
    use std::io::Read;
    let mut linha = Vec::new();
    let mut byte = [0u8; 1];
    let mut linhas = 0;
    while linhas < 12 {
        match saida.read(&mut byte) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                if byte[0] == b'\n' {
                    linhas += 1;
                    if let Some(u) = url_de(&String::from_utf8_lossy(&linha)) {
                        return Some(u);
                    }
                    linha.clear();
                } else {
                    linha.push(byte[0]);
                    // O prompt final não tem `\n`. Sem este corte a leitura ficaria presa
                    // nele até o teto, com a URL já tendo passado — mas ela vem ANTES, e
                    // por isso o corte é rede de segurança e não o caminho normal.
                    if linha.len() > 4096 {
                        break;
                    }
                }
            }
        }
    }
    None
}

/// A extração pura, para ter fixture sem subir processo.
pub(super) fn url_de(linha: &str) -> Option<String> {
    let i = linha.find("https://")?;
    let bruta: String = linha[i..]
        .chars()
        .take_while(|c| !c.is_whitespace())
        .collect();
    // Filtro mínimo: tem de parecer autorização. Sem ele, qualquer link numa mensagem de
    // aviso viraria "a URL do login" e a pessoa autorizaria em outro lugar.
    if bruta.contains("oauth") || bruta.contains("authorize") {
        Some(bruta)
    } else {
        None
    }
}

/// `ler_url` with a deadline (1.6.19).
///
/// 🔴 `ler_url` reads byte by byte until a URL line, 12 lines, 4096 bytes without a newline, or
/// EOF. Its comment spoke of a ceiling ("presa nele até o teto"), but `TETO_DO_LOGIN` only
/// starts in `drenar_e_esperar`, AFTER the URL. A CLI that printed an unrecognized URL and then
/// its prompt — no newline, waiting for the code — blocked the read forever; and since this ran
/// on the UI thread, every window froze, and the designed `sem_url` fallback never fired. On
/// timeout the caller kills the child, which ends the read with EOF.
pub(super) fn ler_url_com_prazo(
    saida: std::process::ChildStdout,
    prazo: std::time::Duration,
) -> Option<(String, std::process::ChildStdout)> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut saida = saida;
        let url = ler_url(&mut saida);
        let _ = tx.send((url, saida));
    });
    match rx.recv_timeout(prazo) {
        Ok((Some(url), saida)) => Some((url, saida)),
        _ => None,
    }
}

/// Asks the person, in a native dialog, before `claude auth login` starts (1.6.38).
///
/// 🔴 `claudeAuthLoginStart` used to spawn the login on the page's word alone — unlike
/// `writeCliConfig`, which asks "Gravar/Cancelar". A script running in the server's origin
/// (XSS, the actor `spawn` already defends against) could start a login, send the URL out and
/// bring a code back, and the profile would then hold someone else's account; for the default
/// profile, so would the `claude` in the terminal. Plausible, not proven end to end. The owner
/// chose "native dialog" (23/09/2026): one more click, and the page cannot answer it.
///
/// Blocks until answered, so it is only called off the UI thread (`blocking_show` on the main
/// thread freezes the app: the dialog needs the event loop that it would be holding).
pub(super) fn confirmar_login(window: &WebviewWindow, perfil: &str, padrao: bool) -> bool {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    window
        .app_handle()
        .dialog()
        .message(texto_da_confirmacao_de_login(perfil, padrao))
        .title("Entrar no Claude Code")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom("Abrir o login".into(), "Cancelar".into()))
        .blocking_show()
}

/// The dialog's text, apart so it can be tested. It names the profile: the decision is which
/// account a directory will hold, and a dialog that does not say which is a click-through.
pub(super) fn texto_da_confirmacao_de_login(perfil: &str, padrao: bool) -> String {
    let perfil = if perfil.trim().is_empty() { "sem nome" } else { perfil.trim() };
    format!(
        "Abrir o login do Claude Code no perfil “{perfil}”?\n\n\
         A conta que você autorizar no navegador passa a ser a deste perfil{terminal}.\n\n\
         Continue só se foi você quem pediu este login agora.",
        terminal = if padrao {
            " — e também a do `claude` no terminal, que usa o mesmo perfil"
        } else {
            ""
        },
    )
}

/// Começa o login e devolve a URL que o cliente imprimiu.
pub(super) fn iniciar_login(
    window: &WebviewWindow,
    conta: Option<&crate::contas_claude::Alvo>,
) -> Result<(String, u64), (&'static str, String)> {
    cancelar_login();
    let bin = resolve_bin("claude")
        .ok_or(("cli_ausente", "claude não encontrado no PATH".to_string()))?;
    let mut cmd = Command::new(bin);
    cmd.arg("auth").arg("login").arg("--claudeai");
    if let Some(p) = crate::user_env::sidecar_path() {
        cmd.env("PATH", p);
    }
    crate::contas_claude::aplicar(&mut cmd, conta);
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut filho = cmd.spawn().map_err(|e| {
        ("spawn_falhou", format!("não consegui iniciar o login: {e}"))
    })?;
    let stdin = filho.stdin.take().ok_or(("spawn_falhou", "sem stdin".to_string()))?;
    let saida = filho.stdout.take().ok_or(("spawn_falhou", "sem stdout".to_string()))?;
    let erro = filho.stderr.take().ok_or(("spawn_falhou", "sem stderr".to_string()))?;

    let Some((url, saida)) = ler_url_com_prazo(saida, PRAZO_URL_DO_LOGIN) else {
        let _ = filho.kill();
        // Reap it: a killed child that is never waited on stays a zombie until the app exits.
        let _ = filho.wait();
        return Err((
            "sem_url",
            "não reconheci a saída do `claude auth login` — rode `claude auth login` no \
             terminal e avise, porque o formato mudou".to_string(),
        ));
    };

    let filho = std::sync::Arc::new(Mutex::new(filho));
    let geracao = PROXIMO_LOGIN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    // The slot is filled BEFORE the waiter starts, so even a CLI that exits at once can
    // only ever clear its own entry.
    if let Ok(mut g) = login_slot().lock() {
        *g = Some(LoginEmCurso { stdin, filho: filho.clone(), geracao });
    }
    drenar_e_esperar(window, filho, saida, erro, geracao);
    Ok((url, geracao))
}

/// Entrega o código colado. **Só escreve** — não espera nada.
///
/// 🔴 Esperar aqui era o defeito: código inválido não encerra o processo (medido: `Invalid
/// code` no stderr e ele segue), então a espera nunca voltava. Quem avisa que acabou é a
/// thread do `drenar_e_esperar`; quem diz se deu certo é o `auth status`.
pub(super) fn entregar_codigo(codigo: &str) -> Result<(), (&'static str, String)> {
    let mut guarda = login_slot()
        .lock()
        .map_err(|_| ("estado_perdido", "estado do login inacessível".to_string()))?;
    let Some(login) = guarda.as_mut() else {
        return Err((
            "sem_login",
            "não há login em curso — comece de novo para gerar uma URL nova".to_string(),
        ));
    };
    // 🔴 O código é ESCRITO e nunca guardado, nem registrado, nem devolvido à tela. Ele vale
    // uma vez e é do usuário; o único lugar a que pertence é o stdin do cliente.
    writeln!(login.stdin, "{codigo}").map_err(|e| {
        ("escrita_falhou", format!("não consegui entregar o código: {e}"))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests_confirmacao_do_login {
    use super::texto_da_confirmacao_de_login;

    /// The dialog names the profile, and says so when the terminal shares it (1.6.38).
    #[test]
    fn o_texto_nomeia_o_perfil() {
        let nomeado = texto_da_confirmacao_de_login("Empresa Blue3", false);
        assert!(nomeado.contains("“Empresa Blue3”"), "{nomeado}");
        assert!(!nomeado.contains("terminal"), "a named profile is not the terminal's: {nomeado}");

        let padrao = texto_da_confirmacao_de_login("Padrão do sistema", true);
        assert!(padrao.contains("“Padrão do sistema”") && padrao.contains("terminal"), "{padrao}");

        assert!(texto_da_confirmacao_de_login("  ", false).contains("“sem nome”"));
    }

    /// SOURCE check, declared as such: the dialog needs a display, so no unit test can click
    /// it. It guards the order the fix depends on, inside the `claudeAuthLoginStart` arm: off
    /// the UI thread first, then the native confirmation, then (and only then) the spawn, with
    /// the refusal returning in between. Needles are built at run time so this test's own text
    /// never matches them.
    #[test]
    fn o_login_so_sobe_depois_do_dialogo() {
        let fonte = include_str!("../code_bridge.rs");
        let ini = fonte.find(&["pub fn handle", "_message("].concat()).expect("handle_message moved");
        let corpo = &fonte[ini..];
        let chave = ["\"claudeAuth", "LoginStart\" =>"].concat();
        let a = corpo.find(&chave).expect("claudeAuthLoginStart arm not found");
        let resto = &corpo[a + chave.len()..];
        let braco = &resto[..resto.find("\n        \"").unwrap_or(resto.len())];

        let pos = |agulha: &str| {
            braco.find(agulha).unwrap_or_else(|| panic!("`{agulha}` is not in the arm:\n{braco}"))
        };
        let despacho = pos(&["fora_da", "_ui("].concat());
        let dialogo = pos(&["confirmar", "_login(w"].concat());
        let recusa = pos(&["\"cancel", "ado\""].concat());
        let login = pos(&["iniciar", "_login(w"].concat());
        assert!(despacho < dialogo, "the dialog blocks: it must run off the UI thread");
        assert!(dialogo < recusa && recusa < login, "the login is spawned before the person says yes");
    }
}

#[cfg(test)]
mod tests_login_geracao {
    #[cfg(unix)]
    use super::{liberar_slot_do_login, login_slot, LoginEmCurso};
    #[cfg(unix)]
    use std::process::{Command, Stdio};

    /// 🔴 1.6.26. The waiter of an OLD login cleared the slot unconditionally, so a restarted
    /// login could be dropped by the previous one's exit. Only the owner clears it now.
    #[cfg(unix)]
    #[test]
    fn o_fim_de_um_login_velho_nao_apaga_o_login_novo() {
        let mut filho = Command::new("cat").stdin(Stdio::piped()).stdout(Stdio::null()).spawn().unwrap();
        let stdin = filho.stdin.take().unwrap();
        let filho = std::sync::Arc::new(std::sync::Mutex::new(filho));
        *login_slot().lock().unwrap() = Some(LoginEmCurso { stdin, filho: filho.clone(), geracao: 7 });

        liberar_slot_do_login(6); // the previous login's waiter finishing late
        assert!(
            login_slot().lock().unwrap().as_ref().is_some_and(|l| l.geracao == 7),
            "the old login's end dropped the new login"
        );
        liberar_slot_do_login(7); // its own waiter
        assert!(login_slot().lock().unwrap().is_none());

        let mut f = filho.lock().unwrap();
        let _ = f.kill();
        let _ = f.wait();
    }
}

#[cfg(test)]
// Split out of `tests_motor` in 1.6.41.
mod tests_login {
    use super::*;

    /// A invariante de proveniência, nos três casos.
    ///
    /// 🔴 O caso do meio é o que mais importa e o menos óbvio: perfil de CREDENCIAL não tem
    /// invariante a priori, porque a variável dele move só a chave e o `configDirectory`
    /// volta sendo a casa padrão **por desenho**. Tratar isso como "não conectado" mandaria
    /// a pessoa refazer um login de pé — o erro que o terceiro estado existe para evitar.
    #[test]
    fn a_proveniencia_separa_o_que_da_para_afirmar_do_que_nao_da() {
        use crate::contas_claude::{Alvo, Var};
        let json = |s: &str| serde_json::json!(s);

        // Sem perfil: nada foi sobrescrito, leitura e turno no mesmo ambiente.
        assert_eq!(proveniencia(None, Some(&json("/Users/x/.claude"))).0, "confirmada");
        assert_eq!(proveniencia(None, None).0, "confirmada");

        // Perfil de credencial: nunca confirmável, e o motivo diz por quê.
        let cred = Alvo { var: Var::SecureStorage, dir: "/Users/x/.claude-cred-b3".into() };
        let (p, motivo) = proveniencia(Some(&cred), Some(&json("/Users/x/.claude")));
        assert_eq!(p, "nao_confirmavel");
        assert!(motivo.unwrap().contains("só a chave"));

        // Casa de configuração: confirma quando bate.
        let casa = Alvo { var: Var::ConfigDir, dir: "/tmp".into() };
        assert_eq!(proveniencia(Some(&casa), Some(&json("/tmp"))).0, "confirmada");

        // …e recusa quando NÃO bate. Este é o caso que a invariante existe para pegar.
        let (p2, m2) = proveniencia(Some(&casa), Some(&json("/Users/x/.claude")));
        assert_eq!(p2, "nao_confirmavel");
        assert!(m2.unwrap().contains("pedi /tmp"));

        // Cliente velho, sem o campo: também é "não sei", nunca "não conectado".
        assert_eq!(proveniencia(Some(&casa), None).0, "nao_confirmavel");
    }

    /// A saída medida do `claude auth login --claudeai` na 2.1.273, VERBATIM.
    ///
    /// Fixture por versão porque isto é saída não documentada: quando ela mudar, este teste
    /// fica vermelho com o texto velho ao lado do novo, em vez de o app mandar a pessoa
    /// autorizar num lugar que não existe mais.
    const SAIDA_2_1_273: &str =
        "Opening browser to sign in…\n\
         If the browser didn't open, visit: https://claude.com/cai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e&response_type=code&redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback&scope=org%3Acreate_api_key&code_challenge=wk8s&code_challenge_method=S256&state=QZiX\n\
         Paste code here if prompted > ";

    #[test]
    fn a_url_sai_da_saida_medida_e_o_desconhecido_vira_recusa() {
        let achada = SAIDA_2_1_273.lines().find_map(url_de);
        let u = achada.expect("a fixture da 2.1.273 tem de render URL");
        assert!(u.starts_with("https://claude.com/cai/oauth/authorize?"));
        // Sem espaço grudado no fim: o prompt vem depois, em outra linha.
        assert!(!u.contains(' '));
        assert!(u.ends_with("state=QZiX"));

        // Tolerante à moldura: a frase em volta já mudou entre versões (o escopo do sign-in
        // mudou na 2.1.273), então casar a frase seria escolher o pedaço frágil.
        assert_eq!(
            url_de("qualquer coisa https://claude.com/cai/oauth/authorize?x=1 e mais"),
            Some("https://claude.com/cai/oauth/authorize?x=1".to_string()),
        );

        // 🔴 E o que NÃO é autorização não vira URL de login. Sem este filtro, um link numa
        // mensagem de aviso viraria "a URL" e a pessoa autorizaria em outro lugar.
        assert_eq!(url_de("veja https://docs.claude.com/erros"), None);
        assert_eq!(url_de("Opening browser to sign in…"), None);
    }

    /// O teto do login é ESCOLHA declarada, e o teste existe para o dia em que virar medição.
    #[test]
    fn o_teto_do_login_e_declarado_e_nao_e_zero() {
        assert!(TETO_DO_LOGIN.as_secs() >= 60, "teto curto demais para uma autorização com 2FA");
        assert!(TETO_DO_LOGIN.as_secs() <= 3600, "teto largo assim é processo pendurado, não teto");
    }

    /// Concluir NÃO espera o filho.
    ///
    /// 🔴 A primeira versão esperava, e a medição mostrou por que não pode: código inválido
    /// não encerra o processo — `Invalid code` sai no stderr e ele segue esperando —, então
    /// a espera nunca voltava. Quem avisa que acabou é a thread que drena.
    #[test]
    fn entregar_codigo_nao_espera_o_filho() {
        let fonte = include_str!("login.rs");
        let corpo = fonte
            .split("fn entregar_codigo")
            .nth(1)
            .and_then(|x| x.split("\n}\n").next()) // to the function's own closing brace (1.6.41)
            .unwrap_or("");
        assert!(!corpo.is_empty(), "a função sumiu — esta régua mediria o nada");
        assert!(corpo.contains("writeln!(login.stdin"));
        for proibido in ["wait_with_output", "wait()", ".wait", "read_to_string"] {
            assert!(!corpo.contains(proibido), "`{proibido}` em entregar_codigo: volta a bloquear");
        }
    }

    /// O que é drenado não chega à tela, e o evento de fim não carrega saída.
    #[test]
    fn a_saida_do_login_nao_vai_para_a_tela() {
        let fonte = include_str!("login.rs");
        let corpo = fonte
            .split("fn drenar_e_esperar")
            .nth(1)
            .and_then(|x| x.split("\n/// Acha a URL").next())
            .unwrap_or("");
        assert!(!corpo.is_empty());
        /* 🔴 Só CÓDIGO. Comentar a linha e a régua continuar verde foi medido: a primeira
         * versão fazia `corpo.contains("lixo.clear()")` e casava dentro de `// lixo.clear();`.
         * É a terceira vez em 16/09 que uma régua que LÊ FONTE casa em texto que não é
         * código — as outras duas foram o lint de bash-4 casando no comentário que explica o
         * defeito, e o `saida:` casando no nome do parâmetro. Régua de fonte sem tirar
         * comentário mede prosa. */
        let so_codigo: String = corpo
            .lines()
            .filter(|l| !l.trim_start().starts_with("//") && !l.trim_start().starts_with("*"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(so_codigo.contains("lixo.clear()"), "sem descartar, o conteúdo fica vivo para vazar");

        /* A régua mira o FORMATO DO EVENTO, não o corpo inteiro: a primeira versão proibia
         * a string `saida:` em qualquer lugar e ficou vermelha com o NOME DO PARÂMETRO
         * `saida: ChildStdout`. Régua que confunde parâmetro com vazamento treina a pessoa
         * a afrouxar a régua. */
        let evento = corpo
            .split("_emit(")
            .nth(1)
            .and_then(|x| x.split(')').next())
            .unwrap_or("");
        assert!(evento.contains("claude_login_fim"), "o evento de fim sumiu");
        assert!(evento.contains("code:"), "sem o código de saída o evento não diz nada");
        for campo in ["saida", "stdout", "stderr", "texto", "lixo", "output"] {
            assert!(
                !evento.contains(campo),
                "`{campo}` no evento: a saída do login iria à tela, e ela é de um cliente que \
                 não promete o que imprime",
            );
        }
    }

    /// O código de autorização é ESCRITO no stdin do cliente e não mora em lugar nenhum.
    ///
    /// Ele vale uma vez, é do usuário, e o único destino legítimo é o processo que o
    /// espera. Este teste lê a própria fonte porque o que ele trava é uma AUSÊNCIA — que
    /// o código não seja guardado em struct, devolvido à tela nem registrado em log — e
    /// ausência não se prova exercitando o caminho feliz.
    #[test]
    fn o_codigo_de_login_nao_e_guardado_nem_devolvido() {
        let fonte = include_str!("login.rs");
        let corpo = fonte
            .split("fn entregar_codigo")
            .nth(1)
            .and_then(|x| x.split("\n}\n").next()) // to the function's own closing brace (1.6.41)
            .unwrap_or("");
        assert!(!corpo.is_empty(), "a função sumiu ou mudou de nome — esta régua mediria o nada");
        assert!(corpo.contains("writeln!(login.stdin"), "o código tem de ir para o stdin do cliente");
        for proibido in ["println!", "eprintln!", "log::", "\"codigo\": codigo", "codigo.to_string()"] {
            assert!(
                !corpo.contains(proibido),
                "`{proibido}` no caminho do código de autorização: ele não pode ser registrado nem devolvido",
            );
        }
    }

    /// Um login por vez, e começar de novo cancela o anterior.
    ///
    /// Dois processos vivos significam dois `code_challenge` (PKCE) diferentes, e o código
    /// que a pessoa colou casaria com um deles por sorte. Um erro que acontece uma vez em
    /// duas é pior que um que acontece sempre — ninguém o reproduz para consertar.
    #[test]
    fn comecar_um_login_cancela_o_anterior() {
        let fonte = include_str!("login.rs");
        let corpo = fonte
            .split("fn iniciar_login")
            .nth(1)
            .and_then(|x| x.split("\n}\n").next()) // `concluir_login` no longer exists: the slice ran to EOF (1.6.41)
            .unwrap_or("");
        assert!(!corpo.is_empty());
        assert!(
            corpo.trim_start().starts_with("cancelar_login();")
                || corpo.contains("cancelar_login();"),
            "iniciar um login sem cancelar o anterior deixa dois PKCE vivos",
        );
    }
}
