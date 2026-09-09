//! Escreve a configuração de um cliente de CLI no host (item **D3**; ADR-026).
//!
//! ## O que faltava
//!
//! O ShvIA-WEB já **gera** a configuração desde a 2.41.0 (Conta → "Conectar meu CLI"):
//! escolhe cliente e modelo e mostra o trecho pronto, com botão Copiar. O que faltava era
//! o passo em que a maioria desiste — **descobrir onde colar**. Cada cliente guarda a
//! config em outro lugar, e um JSON colado no lugar errado, ou colado por cima, apaga o
//! que a pessoa já tinha configurado.
//!
//! ## A regra que decide o desenho: o Rust MONTA, a página não manda arquivo
//!
//! A página envia **valores** (cliente, base, chave, modelo) — nunca o conteúdo do
//! arquivo nem o caminho. O Rust monta o JSON e escolhe o destino de uma lista **fechada**.
//!
//! É a diferença entre "a página propõe uma configuração" e "a página escreve um arquivo
//! arbitrário no seu computador". O `saveFile` da ponte aceita bytes da página porque o
//! destino é escolhido pelo usuário num diálogo de salvar; aqui o destino é um arquivo de
//! configuração **que já existe e importa**, então quem monta é o nativo.
//!
//! ## A validação que mais importa: a base tem de ser o NOSSO servidor
//!
//! Um servidor comprometido que pudesse escolher a `baseUrl` configuraria o Claude Code
//! do usuário para falar com um terceiro — e ele **nunca notaria**, porque o CLI
//! continuaria funcionando. Todo o tráfego de código dele, com prompts e trechos de
//! repositório, passaria pelo atacante.
//!
//! Então a base é conferida contra o mesmo perímetro da navegação
//! ([`crate::is_server_host`]): a lista embutida mais o servidor configurado pelo dono da
//! máquina (item D4). É o mesmo teste que decide se um link abre dentro do app.
//!
//! ## Detecta, mescla, desfaz
//!
//! - **Detecta**: o diretório do cliente existe? Se não, o cliente provavelmente não está
//!   instalado, e criar `~/.continue/` para quem não usa o Continue é sujeira.
//! - **Mescla**: mexe **só** no que é nosso — a entrada de modelo chamada `ShvIA`, ou as
//!   três variáveis `ANTHROPIC_*`. O resto do arquivo volta intacto. Sobrescrever seria
//!   apagar a configuração de outros provedores que a pessoa levou tempo montando.
//! - **Desfaz**: cópia `.shvia-bak` antes de gravar, e o caminho dela volta na resposta.
//!
//! ## E o usuário confirma, no nativo
//!
//! Antes de gravar, um diálogo do SO mostra **o caminho exato**. A página pode propor; só
//! a pessoa autoriza. Sem isso, "o servidor escreve arquivos no seu computador" seria uma
//! frase verdadeira sobre este código.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri::{Manager, WebviewWindow};

/// Teto do que a página pode mandar em cada campo. Chave de API e id de modelo são
/// curtos; um valor de 10 KB só existe para tentar alguma coisa.
const MAX_CAMPO: usize = 512;

/// Cliente suportado. Lista **fechada**: `id` vindo da página escolhe entre estes, e
/// nunca um caminho.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Cliente {
    /// `~/.continue/config.json` — caminho documentado e estável.
    Continue,
    /// `~/.claude/settings.json`, bloco `env`. É o jeito **permanente**; o trecho que a
    /// web mostra hoje manda o usuário editar o `~/.zshrc` na mão.
    ClaudeCode,
    /// `~/.shvia/env.sh` — arquivo NOSSO, para `source`. Não tocamos em `.zshrc`: mexer no
    /// shell de alguém é invasivo, e desfazer viraria adivinhar qual linha era nossa.
    Env,
}

impl Cliente {
    /// `None` para qualquer id fora da lista — inclusive `cline`, `roo` e `curl`, que o
    /// gerador da web cobre mas que **não produzem arquivo**: os dois primeiros são
    /// instruções para a tela de ajustes da extensão e o terceiro é um comando.
    ///
    /// Cline e Roo Code ficam de fora por um motivo concreto, não por esquecimento: a
    /// config deles mora no `globalStorage` de uma extensão do VS Code, cujo caminho
    /// varia por sabor do editor (Code, Insiders, Cursor, VSCodium) e por versão da
    /// extensão. Escrever ali no escuro corromperia o editor de alguém, e detectar isso
    /// com segurança é um item maior que este.
    fn do_id(id: &str) -> Option<Self> {
        match id {
            "continue" => Some(Self::Continue),
            "claude-code" => Some(Self::ClaudeCode),
            "env" => Some(Self::Env),
            _ => None,
        }
    }

    fn rotulo(self) -> &'static str {
        match self {
            Self::Continue => "Continue",
            Self::ClaudeCode => "Claude Code",
            Self::Env => "variáveis de ambiente",
        }
    }

    /// Caminho absoluto, sempre derivado do HOME — nunca de nada que a página mandou.
    ///
    /// `conta_dir` é o diretório de configuração do **perfil de conta do Claude Code**
    /// selecionado (ADR-033), já resolvido por `contas_claude::resolver`. Só o
    /// `ClaudeCode` o usa, e `None` significa a conta `padrao` — o `~/.claude` de sempre.
    ///
    /// 🔴 **Por que isto deixou de ser fixo.** `~/.claude/settings.json` era o destino
    /// cravado, e desde a 1.4.16 o Modo Code escolhe entre perfis de conta. Um usuário na
    /// conta `Empresa · Blue3` que clicasse em "Gravar no meu computador" configuraria o
    /// diretório da conta **errada** — o `env` iria para `~/.claude` e o Code continuaria
    /// lendo `~/.claude-blue3`. Sem erro, e sem sintoma até alguém perguntar por que a
    /// configuração "não pegou". Isto é configuração DO CLAUDE, então ela segue o perfil
    /// de conta do Claude, e não um caminho que era o único que existia quando este
    /// arquivo foi escrito.
    ///
    /// O `conta_dir` vem do registro NATIVO, nunca da página — a página segue mandando só
    /// valores, e nem sabe que perfis existem. O invariante do ADR-026 fica intacto.
    fn caminho(self, home: &Path, conta_dir: Option<&Path>) -> PathBuf {
        match self {
            Self::Continue => home.join(".continue").join("config.json"),
            Self::ClaudeCode => match conta_dir {
                Some(d) => d.join("settings.json"),
                None => home.join(".claude").join("settings.json"),
            },
            Self::Env => home.join(".shvia").join("env.sh"),
        }
    }

    /// O diretório precisa existir antes (= cliente instalado)?
    ///
    /// `false` só para o nosso próprio `~/.shvia`, que criamos. Exigir que
    /// `~/.continue` exista é como se detecta instalação: criar a pasta para quem não usa
    /// o Continue é sujeira que ninguém pediu.
    fn exige_diretorio(self) -> bool {
        self != Self::Env
    }
}

/// Resultado de montar o conteúdo novo.
#[derive(Debug)]
struct Escrita {
    conteudo: String,
    /// Já havia arquivo? Decide backup e permissão.
    existia: bool,
}

/// Entrada do dispatch da ponte (`action: "writeCliConfig"`).
pub fn escrever(window: &WebviewWindow, req: String, v: &Value) {
    let campo = |k: &str| -> String {
        v.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .chars()
            .take(MAX_CAMPO)
            .collect()
    };

    let Some(cliente) = Cliente::do_id(campo("client").trim()) else {
        // Erro explícito, não silêncio: a web precisa poder esconder o botão para os
        // clientes que não produzem arquivo, e uma recusa muda ajudaria a esconder um bug
        // de digitação no id.
        crate::code_bridge::reply(
            window,
            &req,
            false,
            json!({ "error": "cliente sem arquivo de configuração para escrever" }),
        );
        return;
    };

    let base = campo("baseUrl").trim().to_string();
    let chave = campo("apiKey").trim().to_string();
    let modelo = campo("model").trim().to_string();

    // ── A validação que impede o desvio de tráfego ────────────────────────────────
    // Sem ela, um servidor comprometido apontaria o CLI do usuário para um terceiro e
    // nada quebraria — o CLI continuaria respondendo, com o prompt e o código dele
    // passando por fora.
    if !base_confiavel(&base) {
        crate::code_bridge::reply(
            window,
            &req,
            false,
            json!({ "error": "a base precisa ser o servidor deste app, em https" }),
        );
        return;
    }
    if chave.is_empty() || modelo.is_empty() {
        crate::code_bridge::reply(window, &req, false, json!({ "error": "faltou a chave ou o modelo" }));
        return;
    }

    let Ok(home) = window.app_handle().path().home_dir() else {
        crate::code_bridge::reply(window, &req, false, json!({ "error": "não foi possível localizar a sua pasta de usuário" }));
        return;
    };

    /* O perfil de conta do Claude Code manda no destino (ADR-033) — só para o cliente
     * `claude-code`, que é o único cujo arquivo pertence ao Claude.
     *
     * Falha ALTO quando o perfil selecionado não resolve, pela mesma razão do `spawn`:
     * cair no `~/.claude` gravaria a configuração numa conta que o usuário não escolheu, e
     * o sintoma seria "configurei e não pegou" — longe da causa. */
    let conta_dir = if cliente == Cliente::ClaudeCode {
        let (contas, selecionada) = crate::contas_claude::registro(window.app_handle());
        match crate::contas_claude::resolver(&contas, &selecionada) {
            Ok(d) => d,
            Err(e) => {
                crate::code_bridge::reply(
                    window,
                    &req,
                    false,
                    json!({ "error": e.mensagem(), "codigo": e.codigo() }),
                );
                return;
            }
        }
    } else {
        None
    };

    /* 🔴 Só um perfil de CONFIGURAÇÃO tem casa própria para escrever.
     *
     * Um perfil `CLAUDE_SECURESTORAGE_CONFIG_DIR` separa apenas o cofre de credencial e
     * mantém a configuração compartilhada — ele NÃO tem `settings.json` próprio. Gravar
     * dentro do diretório dele poria o arquivo onde o cliente nunca lê: um no-op silencioso,
     * que é exatamente o defeito que a 1.4.19 corrigiu para o outro tipo. Então o destino
     * dele é a casa padrão, que é de fato a que ele usa. */
    let destino = cliente.caminho(
        &home,
        conta_dir
            .as_ref()
            .filter(|a| a.var == crate::contas_claude::Var::ConfigDir)
            .map(|a| a.dir.as_path()),
    );

    // Cinto e suspensório: o caminho é montado aqui, mas conferir que ele está DENTRO do
    // home é o que garante que uma mudança futura no `caminho()` não abra escrita
    // arbitrária sem ninguém notar.
    if !destino.starts_with(&home) {
        crate::code_bridge::reply(window, &req, false, json!({ "error": "destino fora da pasta do usuário" }));
        return;
    }

    let pasta = destino.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| home.clone());
    if cliente.exige_diretorio() && !pasta.is_dir() {
        crate::code_bridge::reply(
            window,
            &req,
            false,
            json!({
                "error": format!(
                    "não encontrei o {} nesta máquina (a pasta {} não existe)",
                    cliente.rotulo(),
                    pasta.display()
                )
            }),
        );
        return;
    }

    let escrita = match montar(cliente, &destino, &base, &chave, &modelo) {
        Ok(e) => e,
        Err(e) => {
            crate::code_bridge::reply(window, &req, false, json!({ "error": e }));
            return;
        }
    };

    confirmar_e_gravar(window, req, cliente, destino, escrita);
}

/// A base é o nosso servidor, em https?
///
/// Reusa o perímetro da navegação de propósito: a pergunta "este endereço é o ShvIA?" já
/// tem uma resposta neste app, e uma segunda lista aqui divergiria da primeira no dia em
/// que um host novo entrasse.
fn base_confiavel(base: &str) -> bool {
    let Ok(u) = tauri::Url::parse(base) else {
        return false;
    };
    if u.scheme() != "https" {
        return false;
    }

    crate::is_server_host(u.host_str().unwrap_or_default())
}

/// Lê o arquivo atual e delega a regra para [`montar_puro`].
fn montar(cliente: Cliente, destino: &Path, base: &str, chave: &str, modelo: &str) -> Result<Escrita, String> {
    let atual = std::fs::read_to_string(destino).ok();

    montar_puro(cliente, destino, atual.as_deref(), base, chave, modelo)
}

/// Monta o conteúdo novo **mesclando** com o que já existe.
///
/// Separado do disco de propósito: a mesclagem é a parte com risco de verdade (apagar a
/// configuração de outro provedor), e ela fica testável sem tocar em arquivo nenhum.
fn montar_puro(
    cliente: Cliente,
    destino: &Path,
    atual: Option<&str>,
    base: &str,
    chave: &str,
    modelo: &str,
) -> Result<Escrita, String> {
    let existia = atual.is_some();

    let conteudo = match cliente {
        Cliente::Env => {
            // Arquivo nosso: reescrito inteiro, e é justamente por isso que ele não é o
            // `.zshrc`.
            format!(
                "# Gerado pelo ShvIA Desktop. Use com: source ~/.shvia/env.sh\n\
                 export OPENAI_BASE_URL=\"{base}\"\n\
                 export OPENAI_API_KEY=\"{chave}\"\n\
                 # modelo sugerido: {modelo}\n"
            )
        }
        Cliente::Continue => {
            let mut raiz = json_existente(atual, destino)?;

            let modelos = raiz
                .as_object_mut()
                .ok_or_else(|| erro_formato(destino))?
                .entry("models")
                .or_insert_with(|| Value::Array(vec![]));
            let lista = modelos.as_array_mut().ok_or_else(|| erro_formato(destino))?;

            // Tira só a NOSSA entrada e recoloca. Um `retain` pelo título é o que faz
            // reexecutar isto ser idempotente em vez de acumular uma entrada "ShvIA" por
            // clique — e o que preserva os outros provedores da pessoa.
            lista.retain(|m| m.get("title").and_then(|t| t.as_str()) != Some("ShvIA"));
            lista.push(json!({
                "title": "ShvIA",
                "provider": "openai",
                "model": modelo,
                "apiBase": base,
                "apiKey": chave,
            }));

            serde_json::to_string_pretty(&raiz).map_err(|e| e.to_string())?
        }
        Cliente::ClaudeCode => {
            let mut raiz = json_existente(atual, destino)?;

            let env = raiz
                .as_object_mut()
                .ok_or_else(|| erro_formato(destino))?
                .entry("env")
                .or_insert_with(|| json!({}));
            let obj = env.as_object_mut().ok_or_else(|| erro_formato(destino))?;

            // Só as três chaves nossas. Qualquer outra variável que a pessoa tenha ali
            // continua onde estava.
            obj.insert("ANTHROPIC_BASE_URL".into(), Value::String(base.to_string()));
            obj.insert("ANTHROPIC_AUTH_TOKEN".into(), Value::String(chave.to_string()));
            obj.insert("ANTHROPIC_MODEL".into(), Value::String(modelo.to_string()));

            serde_json::to_string_pretty(&raiz).map_err(|e| e.to_string())?
        }
    };

    Ok(Escrita { conteudo, existia })
}

/// Lê o JSON existente, ou começa de um objeto vazio.
///
/// **Arquivo com JSON inválido aborta** em vez de ser substituído: um `config.json` que o
/// usuário estava editando e deixou com uma vírgula sobrando não pode ser trocado por um
/// arquivo novo — ele perderia tudo, e o backup não ajudaria porque ele não sabe que
/// existe um.
fn json_existente(atual: Option<&str>, destino: &Path) -> Result<Value, String> {
    match atual {
        None => Ok(json!({})),
        Some(s) if s.trim().is_empty() => Ok(json!({})),
        Some(s) => serde_json::from_str::<Value>(s).map_err(|e| {
            format!(
                "o arquivo {} tem JSON inválido ({e}). Conserte ou renomeie antes — não vou sobrescrevê-lo.",
                destino.display()
            )
        }),
    }
}

fn erro_formato(destino: &Path) -> String {
    format!("o arquivo {} não tem o formato esperado.", destino.display())
}

/// Pede confirmação nativa e grava.
fn confirmar_e_gravar(window: &WebviewWindow, req: String, cliente: Cliente, destino: PathBuf, escrita: Escrita) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

    let win = window.clone();
    let caminho_txt = destino.display().to_string();

    let corpo = format!(
        "Vou gravar a configuração do {} em:\n\n{}\n\n{}",
        cliente.rotulo(),
        caminho_txt,
        if escrita.existia {
            "O arquivo já existe: só as entradas do ShvIA serão trocadas, e faço uma cópia de segurança antes."
        } else {
            "O arquivo será criado."
        },
    );

    window
        .app_handle()
        .dialog()
        .message(corpo)
        .title("Conectar meu CLI")
        // Sem a chave de API no diálogo, de propósito: ela não acrescenta nada à decisão
        // e um print de tela do diálogo vazaria a credencial.
        .buttons(MessageDialogButtons::OkCancelCustom("Gravar".into(), "Cancelar".into()))
        .show(move |ok| {
            if !ok {
                crate::code_bridge::reply(&win, &req, true, json!({ "written": false }));
                return;
            }

            match gravar(&destino, &escrita) {
                Ok(backup) => crate::code_bridge::reply(
                    &win,
                    &req,
                    true,
                    json!({ "written": true, "path": destino.display().to_string(), "backup": backup }),
                ),
                Err(e) => crate::code_bridge::reply(&win, &req, false, json!({ "error": e })),
            }
        });
}

/// Grava, com backup. Devolve o caminho do backup, se houve.
fn gravar(destino: &Path, escrita: &Escrita) -> Result<Option<String>, String> {
    if let Some(dir) = destino.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("não deu para criar {}: {e}", dir.display()))?;
    }

    // Backup ANTES de escrever, e com nome fixo: um `.bak` por gravação encheria a pasta
    // do usuário, e o que ele quer desfazer é sempre a última.
    let mut backup = None;
    if escrita.existia {
        let bak = destino.with_extension(format!(
            "{}shvia-bak",
            destino.extension().map(|_| "").unwrap_or("")
        ));
        let bak = if bak == *destino { destino.with_extension("shvia-bak") } else { bak };
        if std::fs::copy(destino, &bak).is_ok() {
            backup = Some(bak.display().to_string());
        }
    }

    std::fs::write(destino, &escrita.conteudo).map_err(|e| format!("não deu para gravar: {e}"))?;

    // Arquivo NOVO com credencial dentro nasce 0600. Em arquivo que já existia não
    // mexemos na permissão: mudar o modo de um arquivo de configuração de alguém é
    // atrevimento, e pode quebrar o que lê aquilo com outro usuário.
    #[cfg(unix)]
    if !escrita.existia {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(destino, std::fs::Permissions::from_mode(0o600));
    }

    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caminho_falso(nome: &str) -> PathBuf {
        PathBuf::from("/tmp").join(nome)
    }

    /// A lista é FECHADA. `cline` e `roo` são instruções de tela e `curl` é um comando —
    /// aceitar qualquer um deles aqui significaria inventar um arquivo de destino.
    #[test]
    fn so_os_tres_clientes_com_arquivo_entram() {
        assert!(Cliente::do_id("continue").is_some());
        assert!(Cliente::do_id("claude-code").is_some());
        assert!(Cliente::do_id("env").is_some());

        for fora in ["cline", "roo", "curl", "", "../etc/passwd", "CONTINUE"] {
            assert!(Cliente::do_id(fora).is_none(), "'{fora}' não deveria ser aceito");
        }
    }

    /// O caminho vem SEMPRE do home. É o que garante que nada que a página mandou possa
    /// virar destino de escrita.
    #[test]
    fn o_destino_fica_dentro_do_home() {
        let home = PathBuf::from("/Users/alguem");
        for c in [Cliente::Continue, Cliente::ClaudeCode, Cliente::Env] {
            assert!(c.caminho(&home, None).starts_with(&home));
        }
        // Com perfil de conta o destino é o diretório DELE — e ele também vive no home,
        // porque `contas_claude::dir_valido` recusa qualquer coisa fora dali.
        let conta = home.join(".claude-blue3");
        let d = Cliente::ClaudeCode.caminho(&home, Some(&conta));
        assert_eq!(d, conta.join("settings.json"));
        assert!(d.starts_with(&home));
    }

    /// 🔴 O defeito que o `conta_dir` fecha: sem ele, quem estivesse na conta da empresa
    /// gravava a configuração em `~/.claude` e o Modo Code seguia lendo `~/.claude-blue3`.
    /// Nenhum erro, e a descoberta só viria por alguém perguntar por que não pegou.
    #[test]
    fn o_claude_code_segue_o_perfil_de_conta_selecionado() {
        let home = PathBuf::from("/Users/alguem");
        let padrao = Cliente::ClaudeCode.caminho(&home, None);
        let empresa = Cliente::ClaudeCode.caminho(&home, Some(&home.join(".claude-blue3")));
        assert_eq!(padrao, home.join(".claude").join("settings.json"));
        assert_ne!(padrao, empresa, "perfis diferentes não podem ter o mesmo destino");

        // Os outros clientes NÃO seguem a conta: `~/.continue` e `~/.shvia` não são do
        // Claude, e fazer o perfil dele mover o arquivo de outro seria efeito colateral.
        for c in [Cliente::Continue, Cliente::Env] {
            assert_eq!(c.caminho(&home, None), c.caminho(&home, Some(&home.join(".claude-blue3"))));
        }
    }

    /// Mesclar o Continue tem de PRESERVAR os outros provedores. Sobrescrever apagaria a
    /// configuração que a pessoa levou tempo montando — e o pior é que funcionaria, então
    /// ninguém veria erro nenhum.
    #[test]
    fn mesclar_preserva_os_outros_modelos_do_continue() {
        let antes = r#"{"models":[{"title":"GPT local","provider":"ollama"}],"tabAutocompleteModel":{"title":"x"}}"#;
        let e = montar_com(Cliente::Continue, Some(antes)).expect("monta");
        let v: Value = serde_json::from_str(&e.conteudo).expect("json");

        let titulos: Vec<&str> = v["models"].as_array().unwrap().iter().map(|m| m["title"].as_str().unwrap()).collect();
        assert_eq!(titulos, vec!["GPT local", "ShvIA"]);
        // A chave que não é nossa continua ali.
        assert_eq!(v["tabAutocompleteModel"]["title"], "x");
    }

    /// Reexecutar não pode acumular uma entrada "ShvIA" por clique.
    #[test]
    fn escrever_duas_vezes_nao_duplica_a_entrada() {
        let primeira = montar_com(Cliente::Continue, Some("{}")).expect("1ª").conteudo;
        let segunda = montar_com(Cliente::Continue, Some(&primeira)).expect("2ª").conteudo;
        let v: Value = serde_json::from_str(&segunda).expect("json");

        assert_eq!(v["models"].as_array().unwrap().len(), 1);
    }

    /// No Claude Code mexemos em TRÊS variáveis. Qualquer outra que a pessoa tenha no
    /// `env` fica onde estava — e o resto do settings.json também.
    #[test]
    fn mesclar_o_claude_code_toca_so_as_tres_variaveis() {
        let antes = r#"{"env":{"MINHA_VAR":"1"},"permissions":{"allow":["Bash"]}}"#;
        let e = montar_com(Cliente::ClaudeCode, Some(antes)).expect("monta");
        let v: Value = serde_json::from_str(&e.conteudo).expect("json");

        assert_eq!(v["env"]["MINHA_VAR"], "1");
        assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "https://ai.shvia.org/v1");
        assert_eq!(v["permissions"]["allow"][0], "Bash");
    }

    /// JSON inválido ABORTA. Substituir o arquivo faria a pessoa perder a configuração
    /// dela por causa de uma vírgula — e o backup não a salvaria, porque ela não sabe que
    /// existe um.
    #[test]
    fn json_invalido_aborta_em_vez_de_sobrescrever() {
        let r = montar_com(Cliente::Continue, Some(r#"{"models":[,]}"#));

        let erro = r.expect_err("tinha de recusar");
        assert!(erro.contains("JSON inválido"), "mensagem precisa dizer o que houve: {erro}");
        assert!(erro.contains("não vou sobrescrevê-lo"));
    }

    /// Arquivo ausente ou vazio começa de um objeto novo — é o caso da primeira vez.
    #[test]
    fn arquivo_ausente_ou_vazio_comeca_do_zero() {
        assert!(!montar_com(Cliente::Continue, None).expect("ausente").existia);
        assert!(montar_com(Cliente::Continue, Some("   ")).expect("vazio").conteudo.contains("ShvIA"));
    }

    /// A base tem de ser o NOSSO host, em https. Sem isto, um servidor comprometido
    /// apontaria o CLI do usuário para um terceiro e nada quebraria: o CLI continuaria
    /// respondendo, com o prompt e o código dele passando por fora.
    #[test]
    fn a_base_precisa_ser_o_servidor_em_https() {
        assert!(base_confiavel("https://ai.shvia.org/v1"));
        assert!(base_confiavel("https://ia.blue3.com.br/v1"));

        assert!(!base_confiavel("http://ai.shvia.org/v1"), "http não");
        assert!(!base_confiavel("https://evil.com/v1"));
        assert!(!base_confiavel("https://ai.shvia.org.evil.com/v1"));
        assert!(!base_confiavel("https://shvia.org/v1"), "o ápex é a landing, não o app");
        assert!(!base_confiavel("nao-e-url"));
        assert!(!base_confiavel(""));
    }

    /// O `env.sh` é nosso e sai inteiro — com o `source` explicado no cabeçalho, porque
    /// arquivo de export que ninguém carrega não configura nada.
    #[test]
    fn o_env_sh_explica_como_usar() {
        let c = montar_com(Cliente::Env, None).expect("monta").conteudo;

        assert!(c.contains("source ~/.shvia/env.sh"));
        assert!(c.contains("export OPENAI_BASE_URL=\"https://ai.shvia.org/v1\""));
    }

    fn montar_com(cliente: Cliente, atual: Option<&str>) -> Result<Escrita, String> {
        // `montar` lê o arquivo do disco; aqui exercitamos o núcleo passando o conteúdo
        // direto, que é o que tem regra. O caminho só entra nas mensagens de erro.
        montar_puro(cliente, &caminho_falso("config.json"), atual, "https://ai.shvia.org/v1", "sk-teste", "anna")
    }
}
