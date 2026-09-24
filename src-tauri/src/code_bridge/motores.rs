//! The engines: which binary each one is (`motor_do_engine`, one map for `engineStatus` and
//! `spawn`), how it is found on this machine (`resolve_bin`, `engine_status`), the model
//! catalogues, the runner exit codes the page names, and installing the Claude runner from
//! the source that rides in the installer. Split out of `code_bridge.rs` in 1.6.41.

use super::*;

/// Catálogo de modelos do motor Claude Code, perguntado ao próprio SDK.
///
/// Roda `claude-runner --modelos`, que chama `supportedModels()` do Agent SDK e
/// devolve, por modelo, `value`/`displayName`/`description` **e** `supportsEffort`
/// mais `supportedEffortLevels`. É o que permite à UI listar o que existe e desabilitar
/// o que não se aplica, em vez de mostrar o catálogo do gateway (`openai/gpt-5.6-sol`),
/// que não significa nada aqui.
///
/// **Por que perguntar em vez de manter uma lista nossa:** o Claude Code tem
/// aliases próprios (`opus`, `sonnet`, `fable`, `opus[1m]`, `opusplan`, `best`…)
/// que mudam com o cliente. Uma cópia na casa envelheceria em silêncio, e o
/// sintoma seria alguém escolher um modelo que o motor recusa.
///
/// Medido em 21/08: a chamada é de canal de controle e **não consome turno**.
/// Falha nunca é fatal — devolve `erro` e a UI cai no fallback dela; catálogo
/// vazio apresentado como "nenhum modelo" seria pior que dizer que não deu.
///
/// `conta` é o par variável+diretório do perfil escolhido (ADR-033), já resolvido por
/// `contas_claude::resolver` — o **mesmo** resolvedor que o `spawn` usa. O argumento é
/// obrigatório de propósito: enquanto esta função não pedia nada, ela e o `spawn` eram
/// dois caminhos independentes até o mesmo binário, e nada obrigava os dois a concordarem
/// sobre qual conta estava valendo. O catálogo é por assinatura, então discordar aqui
/// significa oferecer na tela um modelo que o turno vai recusar.
pub(super) fn claude_models(conta: Option<&crate::contas_claude::Alvo>) -> serde_json::Value {
    let Some(lancamento) = resolve_runner("claude-runner") else {
        return serde_json::json!({ "erro": erro_runner_ausente(), "codigo": COD_RUNNER_AUSENTE });
    };
    let mut cmd = lancamento.comando();
    cmd.arg("--modelos");
    if let Some(p) = crate::user_env::sidecar_path() {
        cmd.env("PATH", p);
    }
    crate::contas_claude::aplicar(&mut cmd, conta);
    match saida_com_prazo(cmd, PRAZO_CATALOGO) {
        Ok(o) => String::from_utf8_lossy(&o.stdout)
            .lines()
            .find_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .filter(|v| v.get("modelos").is_some())
            .unwrap_or_else(|| serde_json::json!({ "erro": "resposta do claude-runner ilegível" })),
        Err(e) => serde_json::json!({ "erro": format!("falha ao listar modelos: {e}") }),
    }
}

/// Query the same runner and PATH used for Codex turns. The runner bounds the request.
pub(super) fn codex_models() -> serde_json::Value {
    let Some(lancamento) = resolve_runner("codex-runner") else {
        return serde_json::json!({ "erro": erro_codex_ausente(), "codigo": COD_CODEX_AUSENTE });
    };
    let mut cmd = lancamento.comando();
    cmd.arg("--modelos");
    if let Some(p) = crate::user_env::sidecar_path() {
        cmd.env("PATH", p);
    }
    match saida_com_prazo(cmd, PRAZO_CATALOGO) {
        Ok(o) => String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .find(|v| v.get("modelos").is_some() || v.get("erro").is_some())
            .unwrap_or_else(|| serde_json::json!({ "erro": "Atualize o codex-runner para listar os modelos." })),
        Err(e) => serde_json::json!({ "erro": format!("Falha ao listar modelos do Codex: {e}") }),
    }
}

/// Versão do `anna` que ESTE app usaria, com a **origem** junto.
///
/// Existe por causa de 19/08. O Modo Code travava no 422 do gateway com o app na
/// última versão e um `anna 0.8.4` de julho assado dentro dele — e a versão do
/// sidecar não aparecia em lugar nenhum: nem no app, nem na tela de erro, que
/// mandava "atualize o app" enquanto o app já estava atualizado. Diagnóstico que
/// depende de alguém rodar `--version` num binário escondido dentro de um bundle
/// é diagnóstico que ninguém faz.
///
/// A **origem** vai junto porque foi ela que escondeu o caso: o empacotado vence
/// o do PATH (ver `resolve_bin`), então instalar um `anna` novo no PATH não muda
/// nada — e sem essa palavra na tela, a conclusão natural é a errada.
///
/// Roda `--version` na hora do clique. É subprocesso no thread da UI, e o risco
/// aceito é um binário que trave em `--version` travar o modal; na prática ele
/// responde imediato, e é o mesmo caminho que o `stage-anna.mjs` já usa no build.
pub(crate) fn versao_do_anna() -> String {
    let Some(bin) = resolve_bin("anna") else {
        return "não encontrado".into();
    };
    let empacotado = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| bin.starts_with(dir)))
        .unwrap_or(false);
    let origem = if empacotado { "empacotado" } else { "externo" };

    let mut sonda = crate::processo::comando(&bin);
    sonda.arg("--version");
    let bruto = saida_com_prazo(sonda, PRAZO_VERSAO)
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    // O valor é interpolado numa string JS antes do `eval` (ver ABOUT_MODAL_JS):
    // qualquer coisa fora deste conjunto sai, para a saída de um binário nunca
    // conseguir fechar a aspa e virar código.
    let limpo: String = bruto
        .trim_start_matches("anna ")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_'))
        .take(32)
        .collect();

    if limpo.is_empty() {
        format!("? ({origem})")
    } else {
        format!("v{limpo} ({origem})")
    }
}

/// Ausência do `claude-runner`, dita de um jeito que resolve — texto ÚNICO.
///
/// É string de TELA (i18n de produto, por isso em português) e sai dos DOIS pontos que
/// descobrem a ausência: o catálogo (`claude_models`) e o `spawn`. Só o `spawn` dizia o
/// que fazer; o catálogo respondia `claude-runner não encontrado` seco — e o catálogo é o
/// que chega ANTES na tela, porque a página pede a lista de modelos para desenhar o
/// seletor, antes de existir turno. Ou seja: a mensagem sem saída era justamente a que
/// via quem ainda não tinha instalado o runner. Uma constante, e não duas literais, para
/// que a próxima correção do texto não conserte um caminho e deixe o outro para trás.
pub(super) const ERRO_RUNNER_AUSENTE: &str = "claude-runner não encontrado — rode claude-runner/install.sh (deixa em ~/.local/bin) e faça `claude login` (usa a assinatura; sem API key).";

/// Ausência do `codex-runner`, no mesmo molde: diz o que fazer, não só o que falta.
pub(super) const ERRO_CODEX_AUSENTE: &str = "codex-runner não encontrado — rode codex-runner/install.sh (deixa em ~/.local/bin) e faça `codex login` (usa a assinatura ChatGPT; sem API key).";

/// The Windows variants (1.6.57): there the runners come from `install.ps1`, into
/// `%LOCALAPPDATA%\shvia-<runner>`, and the app also installs the Claude one from its button.
/// Screen strings, so Portuguese; picked by `erro_runner_ausente`/`erro_codex_ausente`.
pub(super) const ERRO_RUNNER_AUSENTE_WINDOWS: &str = "claude-runner não encontrado — use o botão Instalar runner ou rode claude-runner\\install.ps1 (instala em %LOCALAPPDATA%\\shvia-claude-runner; precisa do Node 18+) e faça `claude login` (usa a assinatura; sem API key).";
pub(super) const ERRO_CODEX_AUSENTE_WINDOWS: &str = "codex-runner não encontrado — rode codex-runner\\install.ps1 (instala em %LOCALAPPDATA%\\shvia-codex-runner; precisa do Node 18+) e faça `codex login` (usa a assinatura ChatGPT; sem API key).";

/// The absence sentence of this OS. One function per runner, so the catalogue and the
/// `spawn` cannot pick different texts (the reason the constants exist at all).
pub(super) fn erro_runner_ausente() -> &'static str {
    if cfg!(windows) { ERRO_RUNNER_AUSENTE_WINDOWS } else { ERRO_RUNNER_AUSENTE }
}
pub(super) fn erro_codex_ausente() -> &'static str {
    if cfg!(windows) { ERRO_CODEX_AUSENTE_WINDOWS } else { ERRO_CODEX_AUSENTE }
}

/// Ausência do `anna`, que até aqui era uma string solta dentro do `spawn`.
pub(super) const ERRO_ANNA_AUSENTE: &str = "anna não encontrado. Este build saiu SEM o motor empacotado — instale o anna (SHVIA-CODE) e deixe no PATH (Unix: install.sh; Windows: anna.exe no PATH ou %LOCALAPPDATA%\\Programs\\anna)";

/// 🔴 Código estável da ausência, ao lado da frase. **A frase é português; a tela é
/// bilíngue.**
///
/// Medido em 16/09/2026: a página interpola o que a ponte manda no `:erro` de
/// `code.conta_catalogo_falhou`, que ESTÁ catalogado nos dois idiomas. Quem usa a tela em
/// inglês lia *"Could not list the models for this Claude Code account (claude-runner não
/// encontrado — rode claude-runner/install.sh…)"* — metade traduzida, metade não, e a metade
/// não traduzida é justamente a que diz o que fazer.
///
/// Traduzir a constante aqui só inverteria quem fica sem entender. O que a ponte sabe é QUAL
/// ausência é; a língua é assunto da tela, que tem catálogo. Então ela manda o código, e a
/// frase segue junto como reserva — página velha, ou código que a tela ainda não conhece,
/// continua mostrando o que mostrava.
pub(super) const COD_RUNNER_AUSENTE: &str = "runner_ausente";

pub(super) const COD_CODEX_AUSENTE: &str = "codex_ausente";

pub(super) const COD_ANNA_AUSENTE: &str = "anna_ausente";

/// Nome do motor → binário, a frase de ausência dele e o código dela. **Um lugar só.**
///
/// 🔴 Eram DOIS testes binários independentes — `engineStatus` fazia
/// `if base == "claude" { "claude-runner" } else { "anna" }` e o `spawn` repetia a
/// mesma comparação por conta própria. Com dois motores isso funcionava por acidente:
/// qualquer coisa que não fosse `claude` caía no `anna`, e não havia terceira opção
/// para discordarem. Com três, dois testes binários descrevem quatro estados, e o
/// estado "a página pediu `codex` e a ponte spawnou `anna`" é um deles — silencioso,
/// porque o `anna` sobe normalmente e o usuário só descobre pelo comportamento.
///
/// O default continua sendo o `anna`: motor desconhecido não é erro de spawn, é o
/// gateway. Mudar isso quebraria a página velha que não manda `engine`.
pub(super) fn motor_do_engine(engine: &str) -> (&'static str, &'static str, &'static str) {
    match engine {
        "claude" => ("claude-runner", erro_runner_ausente(), COD_RUNNER_AUSENTE),
        "codex" => ("codex-runner", erro_codex_ausente(), COD_CODEX_AUSENTE),
        _ => ("anna", ERRO_ANNA_AUSENTE, COD_ANNA_AUSENTE),
    }
}

/// Código de saída com que o `codex-runner` recusa subir quando a régua de arranque
/// não confirma o sandbox (`codex-runner/codex-runner.mjs`).
///
/// Ele merece nome e tratamento próprios porque **não é "motor indisponível"**: o
/// binário existe, respondeu e decidiu não servir. Contar isso como ausência mandaria
/// o usuário reinstalar algo que está instalado, enquanto o problema real — a garantia
/// do motor não vale nesta máquina — não apareceria em lugar nenhum.
pub(super) const SAIDA_SANDBOX_NAO_CONFIRMADO: i32 = 3;

/// The installer the "Instalar runner" button runs, by OS: `install.sh` with bash, or on
/// Windows (1.6.57) `install.ps1` with Windows PowerShell, which every Windows 10/11 has.
pub(crate) const INSTALADOR: &str = if cfg!(windows) { "install.ps1" } else { "install.sh" };

pub(super) fn script_do_instalador(app: &tauri::AppHandle) -> Result<PathBuf, (&'static str, String)> {
    let dir = app.path().resource_dir().map_err(|e| {
        ("recursos_ausentes", format!("não achei a pasta de recursos do app: {e}"))
    })?;
    /* DOIS candidatos, e isso não é indecisão.
     *
     * O `bundle.resources` lista os arquivos com `../claude-runner/...`, porque a fonte
     * mora fora do `src-tauri`. O Tauri reescreve esse `..` como um segmento literal
     * `_up_` dentro da pasta de recursos — convenção dele, não nossa, e que já mudou entre
     * versões. Apostar num único caminho faria a falha aparecer só no app empacotado, para
     * quem clicou o botão numa máquina sem o repositório: a pessoa com menos condição de
     * diagnosticar, e depois de um release inteiro.
     *
     * Procurar nos dois custa dois `is_file()` e sobrevive à convenção mudar de novo. */
    let candidatos = [
        dir.join("_up_").join("claude-runner").join(INSTALADOR),
        dir.join("claude-runner").join(INSTALADOR),
    ];
    if let Some(script) = candidatos.iter().find(|p| p.is_file()) {
        return Ok(script.clone());
    }
    Err((
        "recursos_ausentes",
        // Os caminhos procurados SAEM na mensagem: um `bundle.resources` com destino
        // trocado é indistinguível de um build velho sem olhar onde se olhou.
        format!(
            "este build não traz a fonte do runner (procurei em {})",
            candidatos.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(" e em "),
        ),
    ))
}

/// Roda o instalador que veio no app (`INSTALADOR`) e devolve a saída dele.
pub(super) fn instalar_claude_runner(app: &tauri::AppHandle) -> Result<String, (&'static str, String)> {
    let script = script_do_instalador(app)?;
    // `-ExecutionPolicy Bypass` applies to THIS run only: a machine on the default
    // (Restricted) policy would refuse the script, and the button is the user's consent.
    let (mut cmd, sem_interprete) = if cfg!(windows) {
        let mut c = crate::processo::comando("powershell");
        c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(&script);
        (c, "powershell_ausente")
    } else {
        let mut c = crate::processo::comando("bash");
        c.arg(&script);
        (c, "bash_ausente")
    };
    // App de GUI não herda o PATH do shell (ADR-029), e o `install.sh` precisa do `node` e
    // do `npm`. Sem isto o botão falharia dizendo que não há Node numa máquina que tem.
    if let Some(p) = crate::user_env::sidecar_path() {
        cmd.env("PATH", p);
    }
    let saida = saida_com_prazo(cmd, PRAZO_INSTALACAO).map_err(|e| {
        if e.kind() == std::io::ErrorKind::TimedOut {
            ("instalacao_sem_resposta", format!("o instalador não terminou: {e}"))
        } else {
            (sem_interprete, format!("não consegui executar o instalador: {e}"))
        }
    })?;
    let texto = format!(
        "{}{}",
        String::from_utf8_lossy(&saida.stdout),
        String::from_utf8_lossy(&saida.stderr),
    );
    if saida.status.success() {
        Ok(texto)
    } else {
        // A saída INTEIRA volta para a tela. O `install.sh` já diz o que falta — Node
        // ausente, import que não copiou, rede — e traduzir isso aqui criaria uma segunda
        // explicação que envelhece separado da primeira.
        Err(("instalacao_falhou", texto))
    }
}

/// How an engine is started: the program, and what goes before the engine's own arguments.
///
/// On Linux and macOS a runner is the wrapper `install.sh` leaves in `~/.local/bin`, and `antes`
/// is empty. On Windows (1.6.57) a runner installed by `install.ps1` is `node <runner>.mjs`, run
/// DIRECTLY, not through the `.cmd` the installer also leaves for terminals: "Parar" kills the
/// child the bridge spawned, and with a `.cmd` that child is `cmd.exe` — `node` would keep the
/// session running after the user stopped it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Lancamento {
    pub(crate) programa: PathBuf,
    pub(crate) antes: Vec<PathBuf>,
}

impl Lancamento {
    pub(crate) fn comando(&self) -> Command {
        let mut c = crate::processo::comando(&self.programa);
        c.args(&self.antes);
        c
    }

    /// What `engineStatus` reports as the path: the runner itself, never `node`.
    pub(crate) fn caminho(&self) -> &std::path::Path {
        self.antes.first().map(|p| p.as_path()).unwrap_or(&self.programa)
    }
}

/// Where `install.ps1` leaves a runner: `%LOCALAPPDATA%\shvia-<runner>\<runner>.mjs`.
pub(crate) fn runner_do_install_ps1(local_app_data: &std::path::Path, base: &str) -> PathBuf {
    local_app_data.join(format!("shvia-{base}")).join(format!("{base}.mjs"))
}

/// The choice itself, with the machine passed in, so it is tested on any OS: CI runs the
/// tests on Linux and macOS, and clippy only on Windows.
///
/// The `install.ps1` folder wins over the binary search on Windows. `where` would find the
/// terminal `.cmd` too, if someone put its folder on PATH, and that `.cmd` is exactly what the
/// bridge must not spawn. Without `node` the install is useless, and the binary search decides.
pub(crate) fn escolher_lancamento(
    base: &str,
    windows: bool,
    local_app_data: Option<&std::path::Path>,
    existe: &dyn Fn(&std::path::Path) -> bool,
    node: &dyn Fn() -> Option<PathBuf>,
    binario: &dyn Fn() -> Option<PathBuf>,
) -> Option<Lancamento> {
    if windows && matches!(base, "claude-runner" | "codex-runner") {
        if let Some(lad) = local_app_data {
            let mjs = runner_do_install_ps1(lad, base);
            if existe(&mjs) {
                if let Some(programa) = node() {
                    return Some(Lancamento { programa, antes: vec![mjs] });
                }
            }
        }
    }
    binario().map(|programa| Lancamento { programa, antes: Vec::new() })
}

/// The launch of an engine on this machine. Every place that starts one goes through here:
/// the session `spawn`, `engineStatus` and the two model catalogues.
pub(super) fn resolve_runner(base: &str) -> Option<Lancamento> {
    let lad = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    escolher_lancamento(
        base,
        cfg!(windows),
        lad.as_deref(),
        &|p| p.is_file(),
        &|| resolve_bin("node"),
        &|| resolve_bin(base),
    )
}

pub(super) fn resolve_bin(base: &str) -> Option<PathBuf> {
    let exe_name = if cfg!(windows) { format!("{base}.exe") } else { base.to_string() };

    // (1) Ao lado do próprio app (bundled).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join(&exe_name);
            if p.exists() {
                return Some(p);
            }
        }
    }

    // (2)+(3) por SO.
    #[cfg(windows)]
    {
        let mut onde = crate::processo::comando("where");
        onde.arg(base);
        if let Ok(out) = saida_com_prazo(onde, PRAZO_VERSAO) {
            if out.status.success() {
                // `where` pode listar vários — pega a 1ª linha.
                if let Some(line) = String::from_utf8_lossy(&out.stdout).lines().next() {
                    let p = PathBuf::from(line.trim());
                    if p.exists() {
                        return Some(p);
                    }
                }
            }
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let p = PathBuf::from(local).join("Programs").join(base).join(&exe_name);
            if p.exists() {
                return Some(p);
            }
        }
    }

    #[cfg(not(windows))]
    {
        let mut probe = crate::processo::comando("sh");
        probe.arg("-c").arg(format!("command -v {base}"));
        if let Some(p) = crate::user_env::sidecar_path() {
            probe.env("PATH", p);
        }
        if let Ok(out) = saida_com_prazo(probe, PRAZO_VERSAO) {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() {
                    return Some(PathBuf::from(p));
                }
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            let p = PathBuf::from(home).join(".local/bin").join(base);
            if p.exists() {
                return Some(p);
            }
        }
    }

    None
}

/// Prontidão do motor: onde ele está e qual versão (item D5).
///
/// O handshake existe porque "o Modo Code não funciona" tem **duas** causas que se
/// parecem na tela — não há binário, ou há um que não roda (arquitetura errada,
/// corrompido, sem permissão de execução). Sem perguntar a versão não dá para
/// distinguir, e o usuário fica tentando reinstalar o que já está lá.
///
/// `--version` com timeout curto: isto é chamado da UI, e um binário travado não
/// pode segurar a tela.
pub fn engine_status(base: &str) -> serde_json::Value {
    let Some(lancamento) = resolve_runner(base) else {
        return serde_json::json!({
            "found": false,
            "bundled": false,
            "version": null,
            "path": null,
        });
    };

    // "Empacotado" = está ao lado do executável do app. É o que o instalador
    // coloca lá, e distinguir isso do que veio do PATH é o que permite dizer ao
    // usuário se ele está rodando o motor testado com esta versão.
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.to_path_buf()))
        .is_some_and(|dir| lancamento.programa.parent() == Some(dir.as_path()));

    let mut sonda = lancamento.comando();
    sonda.arg("--version");
    let versao = saida_com_prazo(sonda, PRAZO_VERSAO)
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());

    serde_json::json!({
        // `found` é o binário EXISTIR; `version` é ele RESPONDER. Os dois separados
        // de propósito: found=true com version=null é o caso "está lá e não roda",
        // que é diagnóstico diferente de "não está lá".
        "found": true,
        "bundled": bundled,
        "version": versao,
        "path": lancamento.caminho().to_string_lossy(),
    })
}

#[cfg(test)]
mod tests_motor {
    use super::*;

    /// 🔴 O mapa tem de ser UM. Até 09/09/2026 a decisão morava em dois `if`
    /// independentes — `engineStatus` e `spawn` — e com dois motores eles não tinham
    /// como discordar: o que não fosse `claude` era `anna`, e ponto. Com três, dois
    /// testes binários descrevem quatro estados, e "a página pediu codex, a ponte
    /// spawnou anna" é um deles. Silencioso, porque o `anna` sobe normalmente.
    #[test]
    fn cada_motor_resolve_para_o_binario_dele() {
        assert_eq!(motor_do_engine("codex").0, "codex-runner");
        assert_eq!(motor_do_engine("claude").0, "claude-runner");
        assert_eq!(motor_do_engine("gateway").0, "anna");
        // Motor desconhecido cai no gateway DE PROPÓSITO: página velha não manda
        // `engine`, e transformar isso em erro de spawn quebraria quem já funciona.
        assert_eq!(motor_do_engine("").0, "anna");
        assert_eq!(motor_do_engine("motor-que-nao-existe").0, "anna");
    }

    /// Cada motor diz o que fazer, não só o que falta — e cada um fala do SEU
    /// instalador. Uma frase genérica mandaria rodar o script errado.
    #[test]
    fn a_ausencia_de_cada_motor_ensina_o_conserto_certo() {
        assert!(motor_do_engine("codex").1.contains("codex-runner/install.sh"));
        assert!(motor_do_engine("codex").1.contains("codex login"));
        assert!(motor_do_engine("claude").1.contains("claude-runner/install.sh"));
        assert!(motor_do_engine("gateway").1.contains("anna"));
        // E nenhuma delas manda o usuário para o instalador do vizinho.
        assert!(!motor_do_engine("codex").1.contains("claude login"));
    }

    /// A fonte do runner está declarada no `bundle.resources`, e o caminho que a ponte
    /// procura tem de casar com o que o Tauri produz.
    ///
    /// Este teste não abre o app empacotado — ele trava a parte que dá para travar sem
    /// bundle: que a busca tem MAIS DE UM candidato, e que o `_up_` (a reescrita do `..`
    /// pelo Tauri, convenção dele e que já mudou entre versões) está entre eles. Um
    /// caminho único aqui faria a falha aparecer só no app instalado, para quem clicou o
    /// botão sem ter o repositório — e depois de um release inteiro.
    #[test]
    fn a_fonte_do_runner_e_procurada_em_mais_de_um_lugar() {
        let conf = include_str!("../../tauri.conf.json");
        assert!(conf.contains("\"resources\""), "sem `bundle.resources` a fonte não viaja");
        assert!(conf.contains("../claude-runner/*.mjs"), "o glob é o que impede a lista de envelhecer");

        let fonte = include_str!("motores.rs");
        let corpo = fonte
            .split("fn script_do_instalador")
            .nth(1)
            .and_then(|x| x.split("fn instalar_claude_runner").next())
            .unwrap_or("");
        assert!(corpo.contains("\"_up_\""), "o caminho que o Tauri produz para `..` não é procurado");
        // Since 1.6.57 the file name is `INSTALADOR` (install.sh, or install.ps1 on Windows):
        // two candidates, each joined to it. Counting "install.sh" would now count nothing.
        assert_eq!(
            corpo.matches(".join(INSTALADOR)").count(),
            2,
            "dois candidatos: procurar num lugar só quebra em silêncio",
        );
        let decl = fonte.lines().find(|l| l.contains("const INSTALADOR")).unwrap_or("");
        assert!(decl.contains("\"install.ps1\"") && decl.contains("\"install.sh\""),
            "o botão precisa de um instalador por SO: {decl}");
        assert!(conf.contains("../claude-runner/install.ps1"), "o install.ps1 não viaja no instalador do app");
    }

    /// 🔴 On Windows a runner is `node <runner>.mjs`, from the `install.ps1` folder, and never
    /// the `.cmd` (1.6.57). Tested with the machine passed in, because CI never runs the tests
    /// on Windows: these cases are the only place the Windows choice is executed before the
    /// owner's runbook.
    #[test]
    fn no_windows_o_runner_e_o_node_com_o_mjs_da_instalacao() {
        use std::path::{Path, PathBuf};
        let lad = Path::new("C:/Users/joão/AppData/Local");
        let mjs = runner_do_install_ps1(lad, "claude-runner");
        assert!(mjs.ends_with("shvia-claude-runner/claude-runner.mjs"), "{mjs:?}");
        let node = || Some(PathBuf::from("C:/Program Files/nodejs/node.exe"));
        let cmd_no_path = || Some(PathBuf::from("C:/Users/joão/AppData/Local/shvia/bin/claude-runner.cmd"));
        let instalado = |p: &Path| p == mjs.as_path();

        // Installed + node: node with the .mjs, even with the terminal .cmd on PATH.
        let l = escolher_lancamento("claude-runner", true, Some(lad), &instalado, &node, &cmd_no_path).unwrap();
        assert_eq!(l.programa, PathBuf::from("C:/Program Files/nodejs/node.exe"));
        assert_eq!(l.antes, vec![mjs.clone()]);
        assert_eq!(l.caminho(), mjs.as_path(), "engineStatus shows the runner, not node");
        let c = l.comando();
        assert_eq!(c.get_program(), std::ffi::OsStr::new("C:/Program Files/nodejs/node.exe"));
        assert_eq!(c.get_args().collect::<Vec<_>>(), vec![mjs.as_os_str()]);

        // Installed, no node: the binary search decides (and finds nothing here).
        let nada = || None;
        assert_eq!(escolher_lancamento("claude-runner", true, Some(lad), &instalado, &nada, &nada), None);
        // Not installed: the binary search decides.
        let ausente = |_: &Path| false;
        let exe = || Some(PathBuf::from("C:/tools/claude-runner.exe"));
        assert_eq!(
            escolher_lancamento("claude-runner", true, Some(lad), &ausente, &node, &exe).unwrap().programa,
            PathBuf::from("C:/tools/claude-runner.exe"),
        );
        // Outside Windows the install.ps1 folder is never looked at.
        let wrapper = || Some(PathBuf::from("/home/x/.local/bin/claude-runner"));
        let l = escolher_lancamento("claude-runner", false, Some(lad), &instalado, &node, &wrapper).unwrap();
        assert!(l.antes.is_empty() && l.programa.as_path() == Path::new("/home/x/.local/bin/claude-runner"));
        // anna is a binary everywhere, installed runner folder or not.
        let anna = || Some(PathBuf::from("C:/app/anna.exe"));
        let tudo = |_: &Path| true;
        assert!(escolher_lancamento("anna", true, Some(lad), &tudo, &node, &anna).unwrap().antes.is_empty());
        // Codex has its own folder.
        assert!(runner_do_install_ps1(lad, "codex-runner").ends_with("shvia-codex-runner/codex-runner.mjs"));
    }

    /// The Windows absence sentences point at install.ps1 and its folder, each with its own login.
    #[test]
    fn no_windows_a_ausencia_ensina_o_install_ps1() {
        assert!(ERRO_RUNNER_AUSENTE_WINDOWS.contains("install.ps1"));
        assert!(ERRO_RUNNER_AUSENTE_WINDOWS.contains("%LOCALAPPDATA%\\shvia-claude-runner"));
        assert!(ERRO_RUNNER_AUSENTE_WINDOWS.contains("claude login") && !ERRO_RUNNER_AUSENTE_WINDOWS.contains("install.sh"));
        assert!(ERRO_CODEX_AUSENTE_WINDOWS.contains("%LOCALAPPDATA%\\shvia-codex-runner"));
        assert!(ERRO_CODEX_AUSENTE_WINDOWS.contains("codex login") && !ERRO_CODEX_AUSENTE_WINDOWS.contains("claude login"));
        // And the picker is what motor_do_engine hands out, on this OS.
        assert_eq!(motor_do_engine("claude").1, erro_runner_ausente());
        assert_eq!(motor_do_engine("codex").1, erro_codex_ausente());
    }

    /// 🔴 O código da ausência é o que a TELA usa para escolher a língua, então ele tem de
    /// ser estável e distinto por motor.
    ///
    /// O par frase+código existe porque a frase é português e a tela é bilíngue: quem usa
    /// em inglês lia a metade que diz o que fazer em português. A ponte não traduz — ela
    /// diz QUAL ausência é, e a tela, que tem catálogo, escolhe a frase.
    ///
    /// A frase continua viajando junto de propósito: página velha, ou código que a tela
    /// ainda não conhece, cai nela em vez de ficar sem mensagem nenhuma.
    #[test]
    fn cada_ausencia_tem_codigo_estavel_e_distinto() {
        assert_eq!(motor_do_engine("claude").2, "runner_ausente");
        assert_eq!(motor_do_engine("codex").2, "codex_ausente");
        assert_eq!(motor_do_engine("gateway").2, "anna_ausente");
        // Motor desconhecido segue o binário: cai no gateway, e o código acompanha.
        assert_eq!(motor_do_engine("").2, "anna_ausente");

        let codigos = ["claude", "codex", "gateway"].map(|e| motor_do_engine(e).2);
        for (i, a) in codigos.iter().enumerate() {
            for b in codigos.iter().skip(i + 1) {
                assert_ne!(a, b, "dois motores com o mesmo código: a tela não tem como separar");
            }
            // Um código vazio seria indistinguível de "a ponte não mandou código", que é
            // exatamente o caso em que a tela DEVE cair na frase.
            assert!(!a.is_empty());
        }
    }

    /// 🔴 O `exit 3` do `codex-runner` NÃO é ausência de motor: o binário existe,
    /// respondeu e recusou servir porque o sandbox não segurou. Se a ponte contar isso
    /// como "motor indisponível", a tela manda reinstalar o que já está instalado e o
    /// motivo real — a garantia não vale nesta máquina — não aparece em lugar nenhum.
    #[test]
    fn a_saida_tres_e_estado_nomeado_e_nao_ausencia() {
        let nomear = |c: Option<i32>| match c {
            Some(SAIDA_SANDBOX_NAO_CONFIRMADO) => "sandbox_nao_confirmado",
            Some(0) | None => "fim",
            Some(_) => "erro",
        };
        assert_eq!(nomear(Some(3)), "sandbox_nao_confirmado");
        assert_eq!(nomear(Some(0)), "fim");
        assert_eq!(nomear(None), "fim");
        assert_eq!(nomear(Some(1)), "erro");
        // O 3 não pode virar "erro" genérico: é essa distinção inteira.
        assert_ne!(nomear(Some(3)), nomear(Some(1)));
    }
}

/// Smoke AO VIVO da ponte: o caminho que o `spawn` usa — `motor_do_engine` →
/// `resolve_bin` → `Command` — encontra o `codex-runner` instalado e conversa com ele.
///
/// 🔴 Compilação verde não conta, e esta frente provou isso seis vezes: cinco defeitos
/// no runner e um na ponte passaram por suíte verde porque nada exercia o processo. Aqui
/// o binário é o de verdade, o `--version` é a mesma sonda que o `engine_status` faz, e o
/// que se prova é o par mapa+resolvedor, não a aritmética de nenhum dos dois.
///
/// `#[ignore]` porque depende de máquina com `codex-runner/install.sh` rodado — ausência
/// dele é ambiente, não regressão, e o CI não teria como distinguir. Roda com
/// `cargo test --lib -- --ignored smoke_codex`.
#[cfg(test)]
mod smoke_codex_ao_vivo {
    use super::*;

    /// 🔴 Esta roda no CI — sem rede, sem login, sem `codex` instalado.
    ///
    /// Ela cobre o modo de falha COMUM, que não é protocolo: o runner não instalado e o
    /// nome errado. O `install.sh` do `claude-runner` já entregou instalação quebrada
    /// uma vez por ter esquecido um arquivo no `cp` — o sintoma foi um motor morto no
    /// primeiro turno do usuário, longe da causa. Aqui isso vira vermelho no push.
    ///
    /// O CI instala o runner antes (`codex-runner/install.sh`: só node e `cp`, com o
    /// schema caindo na cópia do repo quando não há `codex`). Numa máquina de
    /// desenvolvimento sem o motor opcional instalado ela FALHA — e a mensagem diz o
    /// comando. Isso é escolha: o `resolve_bin` procura `~/.local/bin`, então "não
    /// achei" é sempre "não instalei", nunca ambiguidade.
    #[test]
    fn o_codex_runner_esta_instalado_e_responde() {
        let (exe, erro, _) = motor_do_engine("codex");
        assert!(resolve_bin(exe).is_some(), "{erro}");

        let st = engine_status(exe);
        assert_eq!(st["found"], true, "engine_status discordou do resolve_bin: {st}");
        let versao = st["version"].as_str().unwrap_or("");
        assert!(
            !versao.is_empty(),
            "o binário está lá e não respondeu --version — instalação incompleta: {st}",
        );
    }

    #[test]
    #[ignore = "exige codex-runner instalado (codex-runner/install.sh)"]
    fn a_ponte_acha_o_codex_runner_e_ele_responde() {
        let (exe, erro, _) = motor_do_engine("codex");
        let bin = resolve_bin(exe).unwrap_or_else(|| panic!("{erro}"));

        let st = engine_status(exe);
        assert_eq!(st["found"], true, "engine_status não achou o binário que o resolve_bin achou");
        let versao = st["version"].as_str().unwrap_or("");
        assert!(!versao.is_empty(), "o runner não respondeu --version: {st}");

        // E ele fala o NDJSON: um turno vazio (`exit` puro) sobe, prova o sandbox e sai
        // limpo. Se a régua de arranque reprovasse aqui, o código seria 3 — que é
        // exatamente o estado que a ponte agora sabe nomear.
        use std::io::Write;
        use std::process::Stdio;
        let mut filho = crate::processo::comando(&bin)
            .args(["--cwd", std::env::temp_dir().to_str().unwrap_or("/tmp")])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn do codex-runner");
        writeln!(filho.stdin.as_mut().unwrap(), "{{\"type\":\"exit\"}}").ok();
        let saida = filho.wait().expect("wait");
        assert_ne!(
            saida.code(),
            Some(SAIDA_SANDBOX_NAO_CONFIRMADO),
            "o runner recusou subir: o sandbox do Codex não segurou nesta máquina",
        );
    }
}
