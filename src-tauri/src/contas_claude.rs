//! Local registry of **Claude Code account profiles** for Code mode (ADR-033).
//!
//! ## What it is for
//!
//! Someone with more than one Claude Code subscription — one from the company, one
//! personal — switches between them with an environment variable. The Desktop never goes
//! through a shell: it spawns `claude-runner` directly, with no such variable at all. So
//! Code mode could only ever reach whichever account the CLI's default holds, and there
//! was no way to pick.
//!
//! ## ⚠️ Which variable, and on which machine — this module only knows one of the two
//!
//! What is implemented here is `CLAUDE_CONFIG_DIR`, and it was measured on **Linux**
//! (ADR-033, 05/09/2026), where the two profiles are `~/.claude-blue3` and
//! `~/.claude-pessoal` — the two names [`semente`] knows.
//!
//! On the owner's **Mac** that layout does not exist. Measured 08/09/2026: the accounts
//! there are separated by `CLAUDE_SECURESTORAGE_CONFIG_DIR`, which keys the credential
//! store and leaves the configuration home shared. The two variables are **not**
//! interchangeable, and registering a securestorage directory in the `dir` field below
//! fails differently on each operating system — `docs/code/CONTAS-CLAUDE.md` has the
//! measurement and the failure modes.
//!
//! The consequence for a reader of this file: [`semente`] offers nothing on macOS, and
//! [`aplicar`] can only express one of the two separations. That is a known gap, not an
//! oversight — what to do about it is proposed, undecided, in
//! `.continue/contas-claude-macos.md`.
//!
//! ## The rule that decides the design: the page sends an ID, never a path
//!
//! Same line as [ADR-026](../../docs/decisoes.md) (the page proposes values, the native
//! side builds the file) and [ADR-031](../../docs/decisoes.md) (the folder fence comes
//! from the user's gesture, not from the path the page sends). A remote page — or an XSS
//! in it — that could name a directory would point the agent's credentials wherever it
//! liked. It names an **ID from a closed list**; this module resolves the ID.
//!
//! Shell aliases are **not** read. Parsing someone's `.bashrc` to find out what to run is
//! the opposite of a closed list.
//!
//! ## What a profile is, and what it is not
//!
//! A profile is a label plus a configuration directory. `disponivel` means **the directory
//! exists** — nothing more. It is not a claim that the login inside it is valid, has not
//! expired, or belongs to the organization on the label. The labels are the user's own
//! words; this code never verifies an identity and must never render one as verified.
//!
//! Credentials are never read, copied, moved or created here. `claude login` stays with
//! the official client.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

/// Where the registry lives, next to `pastas-autorizadas.json` and `modo-code-bindings.json`.
const ARQUIVO: &str = "contas-claude.json";

/// The ID that means "no `CLAUDE_CONFIG_DIR` at all — whatever the CLI defaults to".
///
/// It is always offered, it has no directory, and it is **never** the destination of a
/// fallback: an unknown or broken ID fails loudly instead of quietly landing here. A named
/// profile that silently resolved to the default would run the turn under an account the
/// user did not choose, and the screen would still say the name they picked.
pub const PADRAO: &str = "padrao";

/// One profile. `dir` is empty exactly for [`PADRAO`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conta {
    pub id: String,
    pub rotulo: String,
    pub dir: String,
}

/// Why an ID did not resolve. The page gets the code, not a filesystem detail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Erro {
    /// The ID is not in the registry.
    Desconhecida,
    /// The ID is known, but its directory is gone.
    Indisponivel,
}

impl Erro {
    pub fn codigo(self) -> &'static str {
        match self {
            Erro::Desconhecida => "conta_desconhecida",
            Erro::Indisponivel => "conta_indisponivel",
        }
    }

    /// User-facing text (Portuguese — product copy, the one carve-out of the language rule).
    pub fn mensagem(self) -> &'static str {
        match self {
            Erro::Desconhecida => "conta desconhecida — escolha uma conta da lista",
            Erro::Indisponivel => {
                "a pasta de configuração desta conta não está mais no lugar — refaça o login \
                 do Claude Code nela ou escolha outra conta"
            }
        }
    }
}

/// The profiles seeded on a machine that has never had this file, given a home directory
/// and the two well-known directories. **Pure**, so the seeding rule has a proof that does
/// not need an `AppHandle` or a real `$HOME`.
///
/// ⚠️ "Well-known" means **known to the Linux machine of ADR-033**, not universal. The two
/// names below are one machine's layout, and a machine that arranges its accounts
/// differently — the owner's Mac does — gets a seed of exactly one entry, the default. The
/// seed is a guess about someone's home directory; it is right to guess conservatively, and
/// it is wrong to present the guess as the only way a profile can come to exist.
///
/// A named profile is seeded **only when its directory already exists**. We do not create
/// directories: an empty `~/.claude-pessoal` invented by us would list as an account and
/// then fail at the first turn, which is worse than not offering it. Absence of the
/// directory is the most reliable "not set up here" signal available without scanning the
/// disk — the same reasoning `cli_config.rs` uses to decide a CLI is not installed.
pub fn semente(home: &Path, existe: &dyn Fn(&Path) -> bool) -> Vec<Conta> {
    let mut contas = vec![Conta {
        id: PADRAO.into(),
        rotulo: "Padrão do sistema".into(),
        dir: String::new(),
    }];
    for (id, rotulo, pasta) in [
        ("empresa-blue3", "Empresa · Blue3", ".claude-blue3"),
        ("pessoal", "Pessoal", ".claude-pessoal"),
    ] {
        let dir = home.join(pasta);
        if existe(&dir) {
            contas.push(Conta {
                id: id.into(),
                rotulo: rotulo.into(),
                dir: dir.to_string_lossy().to_string(),
            });
        }
    }
    contas
}

/// An ID the page may send: lowercase, digits and dashes, starting with an alphanumeric.
///
/// The ID reaches `serde_json` and comes back out in error messages and in the reply the
/// page reads, so the shape is fenced here rather than trusted. It is also the key a
/// hand-edited file can carry, and a key with a slash or a `..` in it is the beginning of
/// a path where a path does not belong.
pub fn id_valido(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Is this a directory we are willing to hand to a child process as `CLAUDE_CONFIG_DIR`?
///
/// Absolute, and inside the user's home. The registry file is hand-editable by design (a
/// second machine registers its own paths there), so what comes off the disk is validated
/// on every read — the same posture as `server::load`, which revalidates the stored URL
/// because the rule may have hardened since it was written.
///
/// `/etc` and friends are out not because reading them would break anything, but because a
/// configuration directory outside the user's home is not a thing this feature has any
/// reason to produce, and the narrow rule is the one that can be checked.
pub fn dir_valido(home: &Path, dir: &str) -> bool {
    let p = Path::new(dir);
    !dir.is_empty() && p.is_absolute() && p.starts_with(home)
}

/// Discards whatever does not survive validation, and guarantees [`PADRAO`] is present.
///
/// Discards rather than repairs: a half-understood entry that we "fixed" would run a turn
/// under a directory nobody wrote down. [`PADRAO`] is re-inserted at the front because a
/// hand-edited file that dropped it would leave the user with no way back to the CLI's own
/// default.
pub fn normalizar(home: &Path, contas: Vec<Conta>) -> Vec<Conta> {
    let mut saida: Vec<Conta> = Vec::new();
    for c in contas {
        if c.id == PADRAO {
            continue; // reinserido abaixo, com o rótulo e o dir vazio canônicos
        }
        if !id_valido(&c.id) || !dir_valido(home, &c.dir) || c.rotulo.trim().is_empty() {
            continue;
        }
        if saida.iter().any(|x: &Conta| x.id == c.id) {
            continue;
        }
        saida.push(c);
    }
    saida.insert(
        0,
        Conta {
            id: PADRAO.into(),
            rotulo: "Padrão do sistema".into(),
            dir: String::new(),
        },
    );
    saida
}

/// Resolve an ID to the directory a child process should get.
///
/// `Ok(None)` is [`PADRAO`]: the child inherits nothing and the CLI picks its own default.
/// `Ok(Some(dir))` is a named profile whose directory is there right now — checked at
/// resolve time, not at load time, because the user may have moved it since the app opened.
///
/// **There is no fallback arm, and that is the point.** Both `claude_models` and `spawn`
/// call this one function, so discovery and the turn cannot end up on different accounts —
/// which is the failure this whole feature exists to make impossible.
pub fn resolver(contas: &[Conta], id: &str) -> Result<Option<PathBuf>, Erro> {
    let alvo = if id.trim().is_empty() { PADRAO } else { id.trim() };
    let conta = contas
        .iter()
        .find(|c| c.id == alvo)
        .ok_or(Erro::Desconhecida)?;
    if conta.dir.is_empty() {
        return Ok(None);
    }
    let p = PathBuf::from(&conta.dir);
    if p.is_dir() {
        Ok(Some(p))
    } else {
        Err(Erro::Indisponivel)
    }
}

/// Applies the resolved directory to **one child process**.
///
/// 🔴 `Command::env`, never `std::env::set_var`. Two windows can be on two accounts at the
/// same time; a process-wide variable would make whichever window spawned last decide for
/// the other, and the other window's screen would keep showing the account it had picked.
pub fn aplicar(cmd: &mut std::process::Command, dir: Option<&Path>) {
    if let Some(d) = dir {
        cmd.env("CLAUDE_CONFIG_DIR", d);
    }
}

// ── disco ──────────────────────────────────────────────────────────────────────

fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn arquivo(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join(ARQUIVO))
}

fn do_json(v: &serde_json::Value) -> (Vec<Conta>, String) {
    let contas = v
        .get("contas")
        .and_then(|c| c.as_array())
        .map(|a| {
            a.iter()
                .map(|c| Conta {
                    id: c.get("id").and_then(|x| x.as_str()).unwrap_or_default().into(),
                    rotulo: c.get("rotulo").and_then(|x| x.as_str()).unwrap_or_default().into(),
                    dir: c.get("dir").and_then(|x| x.as_str()).unwrap_or_default().into(),
                })
                .collect()
        })
        .unwrap_or_default();
    let selecionada = v
        .get("selecionada")
        .and_then(|x| x.as_str())
        .unwrap_or(PADRAO)
        .to_string();
    (contas, selecionada)
}

fn para_json(contas: &[Conta], selecionada: &str) -> serde_json::Value {
    serde_json::json!({
        "contas": contas
            .iter()
            .filter(|c| c.id != PADRAO) // o padrão é implícito; gravá-lo convidaria a editá-lo
            .map(|c| serde_json::json!({ "id": c.id, "rotulo": c.rotulo, "dir": c.dir }))
            .collect::<Vec<_>>(),
        "selecionada": selecionada,
    })
}

/// The registry as it stands: profiles plus the persisted selection.
///
/// **Never fails.** A missing file seeds from the well-known directories and writes it once
/// (the same shape as `pastas_autorizadas`); corrupted JSON falls back to that seed. A
/// registry that could not be read must not be a Code mode that will not open.
///
/// The persisted selection is revalidated: a profile removed from the file by hand, or a
/// directory that moved, leaves the selection pointing at nothing, and the answer is
/// [`PADRAO`] — the one place falling back to the default is right, because no turn has
/// been launched yet and the user is about to see which account is selected.
pub fn registro(app: &AppHandle) -> (Vec<Conta>, String) {
    let casa = home();
    let caminho = arquivo(app);

    if let Some(bruto) = caminho
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    {
        let (contas, selecionada) = do_json(&bruto);
        let contas = normalizar(&casa, contas);
        let selecionada = if contas.iter().any(|c| c.id == selecionada) {
            selecionada
        } else {
            PADRAO.to_string()
        };
        return (contas, selecionada);
    }

    let contas = semente(&casa, &|p| p.is_dir());
    if let (Some(p), Ok(s)) = (
        caminho,
        serde_json::to_string_pretty(&para_json(&contas, PADRAO)),
    ) {
        let _ = std::fs::write(p, s);
    }
    (contas, PADRAO.to_string())
}

/// Persists the selected ID. Rejects an ID that is not in the registry — the selection is
/// state the page proposes, and the page does not get to invent a profile by picking one.
pub fn selecionar(app: &AppHandle, id: &str) -> Result<String, Erro> {
    let (contas, _) = registro(app);
    let conta = contas.iter().find(|c| c.id == id).ok_or(Erro::Desconhecida)?;
    let escolhida = conta.id.clone();
    if let (Some(p), Ok(s)) = (
        arquivo(app),
        serde_json::to_string_pretty(&para_json(&contas, &escolhida)),
    ) {
        let _ = std::fs::write(p, s);
    }
    Ok(escolhida)
}

/// The registry as the page sees it: IDs, labels, whether the directory is there, and the
/// selection. No paths — the page has no use for one and cannot send one back.
pub fn como_json(app: &AppHandle) -> serde_json::Value {
    let (contas, selecionada) = registro(app);
    serde_json::json!({
        "contas": contas
            .iter()
            .map(|c| {
                let disponivel = c.dir.is_empty() || Path::new(&c.dir).is_dir();
                serde_json::json!({
                    "id": c.id,
                    "rotulo": c.rotulo,
                    "disponivel": disponivel,
                    "motivo": if disponivel { serde_json::Value::Null }
                              else { serde_json::json!(Erro::Indisponivel.mensagem()) },
                })
            })
            .collect::<Vec<_>>(),
        "selecionada": selecionada,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn casa() -> PathBuf {
        PathBuf::from("/home/dev")
    }

    fn conta(id: &str, dir: &str) -> Conta {
        Conta { id: id.into(), rotulo: "Rótulo".into(), dir: dir.into() }
    }

    #[test]
    fn a_semente_so_traz_a_conta_cuja_pasta_existe() {
        // Nenhuma das duas no disco: sobra o padrão, e ele sozinho é resposta legítima.
        let so_padrao = semente(&casa(), &|_| false);
        assert_eq!(so_padrao.len(), 1);
        assert_eq!(so_padrao[0].id, PADRAO);
        assert!(so_padrao[0].dir.is_empty());

        // Só a da empresa: a pessoal NÃO entra, e nada é criado para fazê-la entrar.
        let so_empresa = semente(&casa(), &|p| p.ends_with(".claude-blue3"));
        assert_eq!(so_empresa.len(), 2);
        assert_eq!(so_empresa[1].id, "empresa-blue3");
        assert_eq!(so_empresa[1].dir, "/home/dev/.claude-blue3");

        let ambas = semente(&casa(), &|_| true);
        assert_eq!(
            ambas.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec![PADRAO, "empresa-blue3", "pessoal"],
        );
    }

    #[test]
    fn id_desconhecido_falha_em_vez_de_cair_no_padrao() {
        let contas = semente(&casa(), &|_| false);
        assert_eq!(resolver(&contas, "empresa-blue3"), Err(Erro::Desconhecida));
        // 🔴 O caso que a ausência de fallback protege: um id que não existe NÃO pode
        // devolver `Ok(None)`, senão o turno roda no padrão com a tela dizendo outra conta.
        assert_ne!(resolver(&contas, "empresa-blue3"), Ok(None));
    }

    #[test]
    fn o_padrao_resolve_para_nenhum_diretorio() {
        let contas = semente(&casa(), &|_| false);
        assert_eq!(resolver(&contas, PADRAO), Ok(None));
        // String vazia é o mesmo pedido: a página que não manda `accountId` quer o padrão.
        assert_eq!(resolver(&contas, ""), Ok(None));
        assert_eq!(resolver(&contas, "  "), Ok(None));
    }

    #[test]
    fn conta_com_pasta_que_sumiu_e_indisponivel() {
        let contas = vec![conta("pessoal", "/home/dev/.claude-que-nao-existe")];
        assert_eq!(resolver(&contas, "pessoal"), Err(Erro::Indisponivel));
    }

    #[test]
    fn conta_cuja_pasta_existe_resolve_para_ela() {
        // Diretório de verdade num temporário — o mesmo critério do resto deste crate:
        // o que se prova é que `is_dir()` decide, e um mock provaria só a nossa aritmética.
        let dir = std::env::temp_dir().join(format!("shvia-conta-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let c = vec![conta("x", &dir.to_string_lossy())];
        assert_eq!(resolver(&c, "x"), Ok(Some(dir.clone())));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn id_fora_do_formato_e_descartado_na_leitura() {
        assert!(id_valido("empresa-blue3"));
        assert!(id_valido("pessoal"));
        assert!(!id_valido(""));
        assert!(!id_valido("Empresa"));       // maiúscula
        assert!(!id_valido("../etc"));        // caminho onde não cabe caminho
        assert!(!id_valido("com espaço"));
        assert!(!id_valido("-comeca-com-traco"));
        assert!(!id_valido(&"a".repeat(33)));
    }

    #[test]
    fn diretorio_fora_da_casa_do_usuario_e_recusado() {
        assert!(dir_valido(&casa(), "/home/dev/.claude-blue3"));
        assert!(!dir_valido(&casa(), "/etc/claude"));
        assert!(!dir_valido(&casa(), ".claude-blue3")); // relativo
        assert!(!dir_valido(&casa(), ""));
        // Vizinho de nome parecido: `starts_with` de `Path` compara COMPONENTE, então
        // `/home/dev2` não cai dentro de `/home/dev` — o furo de um prefixo de string.
        assert!(!dir_valido(&casa(), "/home/dev2/.claude-blue3"));
    }

    #[test]
    fn normalizar_descarta_o_invalido_e_garante_o_padrao() {
        let entrada = vec![
            conta("empresa-blue3", "/home/dev/.claude-blue3"),
            conta("MAIUSCULA", "/home/dev/.claude-x"),
            conta("fora", "/etc/claude"),
            conta("empresa-blue3", "/home/dev/.claude-outra"), // duplicata: a 1ª vence
            Conta { id: "sem-rotulo".into(), rotulo: "  ".into(), dir: "/home/dev/.c".into() },
        ];
        let saida = normalizar(&casa(), entrada);
        assert_eq!(
            saida.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec![PADRAO, "empresa-blue3"],
        );
        assert_eq!(saida[1].dir, "/home/dev/.claude-blue3");
    }

    #[test]
    fn o_padrao_volta_mesmo_se_o_arquivo_o_tiver_perdido() {
        // Arquivo editado à mão sem o padrão: sem esta reinserção o usuário ficaria sem
        // caminho de volta para o diretório default do CLI.
        let saida = normalizar(&casa(), vec![conta("pessoal", "/home/dev/.claude-pessoal")]);
        assert_eq!(saida[0].id, PADRAO);
        assert!(saida[0].dir.is_empty());
    }

    #[test]
    fn aplicar_so_seta_a_variavel_quando_ha_diretorio() {
        let mut cmd = std::process::Command::new("true");
        aplicar(&mut cmd, None);
        assert!(cmd.get_envs().next().is_none(), "o padrão não pode setar CLAUDE_CONFIG_DIR");

        let mut cmd = std::process::Command::new("true");
        aplicar(&mut cmd, Some(Path::new("/home/dev/.claude-blue3")));
        let envs: Vec<_> = cmd.get_envs().collect();
        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].0, std::ffi::OsStr::new("CLAUDE_CONFIG_DIR"));
        assert_eq!(envs[0].1, Some(std::ffi::OsStr::new("/home/dev/.claude-blue3")));
    }

    #[test]
    fn o_json_gravado_nao_carrega_o_padrao_e_guarda_a_selecao() {
        let contas = semente(&casa(), &|_| true);
        let v = para_json(&contas, "pessoal");
        let gravadas = v["contas"].as_array().unwrap();
        assert_eq!(gravadas.len(), 2, "o padrão é implícito, não gravado");
        assert_eq!(v["selecionada"], "pessoal");

        // Ida e volta: o que sai do disco reconstrói o que entrou.
        let (lidas, sel) = do_json(&v);
        assert_eq!(sel, "pessoal");
        assert_eq!(normalizar(&casa(), lidas), contas);
    }
}
