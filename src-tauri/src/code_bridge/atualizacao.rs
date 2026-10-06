//! The runners follow the app (1.7.1), and since 1.13.0 they also COME with it (ADR-040).
//!
//! ## Why
//!
//! The app updates itself; the runners did not. They live outside it — the folders `install.sh`
//! leaves in `~/.local/share` (or `$XDG_DATA_HOME`), `install.ps1` in `%LOCALAPPDATA%` — and
//! changed only when someone ran an installer by hand. Measured on the owner's machine on
//! 25/09/2026, with the app at 1.6.58: the `codex-runner` was 1.4.34 (09/09) and did not know
//! `--modelos`, so the Codex engine listed no model and no effort and locked both selectors; the
//! `claude-runner` was from 21/08, a month of fixes behind. The app could not even have fixed the
//! Codex one: it shipped only the Claude runner.
//!
//! Now it ships both, and at every start compares each installed runner with the one it brought.
//!
//! ## The rule (`decidir`)
//!
//! - **missing** → install, in the background, with the bundled installer (1.13.0, ADR-040). Until
//!   1.12.4 this row said "nothing — the first install stays the button's, the user's consent", and
//!   the owner measured what that costs: updated the app to the latest version, picked the Code
//!   engine, and the model catalogue and the account selector were empty, because the runner had
//!   never been installed and nothing said so where he was looking. The consent the row protected
//!   is replaced by a notification that says what is being downloaded and how to stop it
//!   (`SHVIA_RUNNERS_AUTO=0`). Needs Node 18+ (the installer's own requirement): without it the
//!   person is told once, in words, instead of finding an empty selector;
//! - **found somewhere else** (a developer's checkout on PATH, an install in another folder) →
//!   nothing: the app installs what is MISSING, never over what someone put there on purpose;
//! - **older** than the bundled one → reinstall, with the bundled installer;
//! - **same version, different files** → reinstall. The `codex-runner` has its own version, and
//!   measured on 25/09 it changed 8 times since 09/09 with 7 bumps: equal numbers do not prove equal
//!   runners;
//! - **newer** → nothing. A runner someone installed by hand from a newer checkout is not the
//!   app's to downgrade.
//!
//! The reinstall runs the SAME installer the button runs (`instalar_runner`), never a copy of its
//! steps: two definitions of an install would drift, and the one a developer tests in a terminal
//! would stop being the one the user gets.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, TryLockError};

use tauri::Manager;
use tauri_plugin_notification::NotificationExt;

/// The runners the app ships and keeps current.
pub(crate) const RUNNERS: [&str; 2] = ["claude-runner", "codex-runner"];

/// One install of a runner at a time — the button's and this one's alike: two would write the
/// same folder at once. Poisoning is ignored: a panic mid-install leaves nothing to protect.
fn trava(base: &str) -> &'static Mutex<()> {
    static CLAUDE: Mutex<()> = Mutex::new(());
    static CODEX: Mutex<()> = Mutex::new(());
    static OUTRO: Mutex<()> = Mutex::new(());
    match base {
        "claude-runner" => &CLAUDE,
        "codex-runner" => &CODEX,
        _ => &OUTRO,
    }
}

/// Holds the runner's install lock, or `None` when an install is already running.
pub(crate) fn tentar_instalar(base: &str) -> Option<MutexGuard<'static, ()>> {
    match trava(base).try_lock() {
        Ok(g) => Some(g),
        Err(TryLockError::Poisoned(p)) => Some(p.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

/// Waits for an install of this runner to end. For the callers that start the runner OFF the UI
/// thread (the model catalogues): they get the new runner instead of one half copied.
pub(crate) fn esperar_instalacao(base: &str) {
    drop(trava(base).lock().unwrap_or_else(|p| p.into_inner()));
}

/// Whether an install of this runner is running right now, without waiting.
pub(crate) fn instalando(base: &str) -> bool {
    matches!(trava(base).try_lock(), Err(TryLockError::WouldBlock))
}

/// What the machine has of a runner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Estado {
    /// The app finds the runner and the installer's folder exists: the app's own install.
    Instalado,
    /// The app finds NO runner anywhere.
    Ausente,
    /// The app finds a runner, but not in the folder the installers use: someone put it there.
    EmOutroLugar,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Decisao {
    /// The bundle carries no source of this runner (a build that left it out): nothing to install from.
    SemFonte,
    Instalar { para: String },
    ForaDoPadrao,
    EmDia,
    MaisNovo { instalada: String, empacotada: String },
    Atualizar { de: String, para: String },
}

/// `x.y.z` compared as numbers; a part that is not a number counts as 0.
fn comparar(a: &str, b: &str) -> Ordering {
    let partes = |v: &str| -> Vec<u64> { v.trim().split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    let (pa, pb) = (partes(a), partes(b));
    for i in 0..pa.len().max(pb.len()) {
        match pa.get(i).unwrap_or(&0).cmp(pb.get(i).unwrap_or(&0)) {
            Ordering::Equal => continue,
            outra => return outra,
        }
    }
    Ordering::Equal
}

/// The rule, with everything passed in (see the module doc). An installed runner whose version
/// cannot be read counts as older: every installer since the first copies its `package.json`.
pub(crate) fn decidir(estado: Estado, versao_instalada: Option<&str>, empacotada: &str, arquivos_iguais: bool) -> Decisao {
    match estado {
        Estado::Ausente => return Decisao::Instalar { para: empacotada.to_string() },
        Estado::EmOutroLugar => return Decisao::ForaDoPadrao,
        Estado::Instalado => {}
    }
    let de = versao_instalada.unwrap_or("?").to_string();
    match versao_instalada.map(|v| comparar(v, empacotada)) {
        None | Some(Ordering::Less) => Decisao::Atualizar { de, para: empacotada.to_string() },
        Some(Ordering::Greater) => Decisao::MaisNovo { instalada: de, empacotada: empacotada.to_string() },
        Some(Ordering::Equal) if !arquivos_iguais => Decisao::Atualizar { de, para: empacotada.to_string() },
        Some(Ordering::Equal) => Decisao::EmDia,
    }
}

/// Where the installers leave a runner's files: `install.sh` in `$XDG_DATA_HOME` (or
/// `~/.local/share`), `install.ps1` in `%LOCALAPPDATA%`, each in `shvia-<runner>`.
pub(crate) fn pasta_instalada(base: &str, windows: bool, env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let raiz = if windows {
        PathBuf::from(env("LOCALAPPDATA").filter(|s| !s.is_empty())?)
    } else {
        match env("XDG_DATA_HOME").filter(|s| !s.is_empty()) {
            Some(xdg) => PathBuf::from(xdg),
            None => PathBuf::from(env("HOME").filter(|s| !s.is_empty())?).join(".local").join("share"),
        }
    };
    Some(raiz.join(format!("shvia-{base}")))
}

/// Where the bundle carries a runner, by the same two candidates the installer search uses
/// (`script_do_instalador`): Tauri rewrites `..` as `_up_`, and that convention has changed before.
pub(crate) fn pasta_empacotada(recursos: &Path, base: &str) -> Option<PathBuf> {
    [recursos.join("_up_").join(base), recursos.join(base)]
        .into_iter()
        .find(|p| p.join("package.json").is_file())
}

/// The `version` of the `package.json` in a runner folder.
pub(crate) fn versao_em(pasta: &Path) -> Option<String> {
    let texto = std::fs::read_to_string(pasta.join("package.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&texto).ok()?;
    v.get("version").and_then(|x| x.as_str()).map(str::to_string)
}

/// Files a runner uses from OUTSIDE its folder, as a path relative to it. The Codex runner imports
/// `../claude-runner/politica.mjs`, and its installers keep that copy: without comparing it, a
/// policy change that did not bump the Codex version would never reach the Codex engine.
fn arquivos_de_fora(base: &str) -> &'static [&'static str] {
    match base {
        "codex-runner" => &["../claude-runner/politica.mjs"],
        _ => &[],
    }
}

/// Whether the runner files the app brought have the same bytes as the installed ones.
///
/// Only files present on BOTH sides are compared. Each installer copies a subset of what the bundle
/// carries (tests, READMEs and installers stay behind), and a file the installer never copies would
/// otherwise look missing at every start and reinstall forever.
pub(crate) fn arquivos_iguais(empacotada: &Path, instalada: &Path, base: &str) -> bool {
    let Ok(entradas) = std::fs::read_dir(empacotada) else {
        return false;
    };
    let mut pares: Vec<(PathBuf, PathBuf)> = entradas
        .flatten()
        .filter_map(|e| {
            let nome = e.file_name().to_string_lossy().to_string();
            let conta = (nome.ends_with(".mjs") && !nome.ends_with(".test.mjs")) || nome == "package.json";
            conta.then(|| (e.path(), instalada.join(&nome)))
        })
        .collect();
    for rel in arquivos_de_fora(base) {
        pares.push((empacotada.join(rel), instalada.join(rel)));
    }
    pares
        .into_iter()
        .filter(|(de, para)| de.is_file() && para.is_file())
        .all(|(de, para)| matches!((std::fs::read(&de), std::fs::read(&para)), (Ok(a), Ok(b)) if a == b))
}

/// Whether the app may install a runner that is MISSING, decided by the caller (it needs the machine:
/// an environment variable and a `node --version`) and asked only when a runner is actually missing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PrimeiraInstalacao {
    Permitida,
    /// The person switched it off (`SHVIA_RUNNERS_AUTO=0`).
    Desligada,
    /// No Node 18+ the installer could use: it would fail, and a failed install says less than a sentence.
    SemNode,
}

/// What happened to one runner at a start, for the notification and the log.
#[derive(Debug, PartialEq)]
pub(crate) enum Resultado {
    Nada(Decisao),
    Atualizado { de: String, para: String },
    Falhou { de: String, para: String, erro: String },
    Instalado { para: String },
    FalhouAoInstalar { para: String, erro: String },
    FaltaNode { para: String },
    Desligado,
    OcupadoPeloBotao,
}

/// One runner, with the machine passed in: `estado` is what the app finds of it; `primeira` says whether a
/// MISSING one may be installed (called only then); `ao_comecar` runs just before a first install, so the
/// person is told it started; `instalar` runs the bundled installer. Holds the install lock while it runs.
pub(crate) fn atualizar_se_preciso(
    base: &str,
    empacotada: &Path,
    instalada: &Path,
    estado: Estado,
    primeira: &dyn Fn() -> PrimeiraInstalacao,
    ao_comecar: &dyn Fn(),
    instalar: &dyn Fn() -> Result<String, String>,
) -> Resultado {
    let Some(versao_empacotada) = versao_em(empacotada) else {
        return Resultado::Nada(Decisao::SemFonte);
    };
    let decisao = decidir(
        estado,
        versao_em(instalada).as_deref(),
        &versao_empacotada,
        arquivos_iguais(empacotada, instalada, base),
    );
    match decisao {
        Decisao::Instalar { para } => {
            match primeira() {
                PrimeiraInstalacao::Desligada => return Resultado::Desligado,
                PrimeiraInstalacao::SemNode => return Resultado::FaltaNode { para },
                PrimeiraInstalacao::Permitida => {}
            }
            let Some(_trava) = tentar_instalar(base) else {
                return Resultado::OcupadoPeloBotao;
            };
            ao_comecar();
            match instalar() {
                Ok(_) => Resultado::Instalado { para },
                Err(erro) => Resultado::FalhouAoInstalar { para, erro },
            }
        }
        Decisao::Atualizar { de, para } => {
            let Some(_trava) = tentar_instalar(base) else {
                return Resultado::OcupadoPeloBotao;
            };
            match instalar() {
                Ok(_) => Resultado::Atualizado { de, para },
                Err(erro) => Resultado::Falhou { de, para, erro },
            }
        }
        outra => Resultado::Nada(outra),
    }
}

/// The installer's own requirement, read from `node --version` (`v24.15.0`): 18 or newer.
pub(crate) fn versao_do_node_serve(saida: &str) -> bool {
    saida
        .trim()
        .trim_start_matches('v')
        .split('.')
        .next()
        .and_then(|m| m.parse::<u32>().ok())
        .is_some_and(|m| m >= 18)
}

/// Whether the machine has a Node the installer can use. Asked the way the installer asks: with the PATH a
/// shell would give (a GUI app does not inherit it, ADR-029), and with a deadline.
fn node_serve() -> bool {
    let mut cmd = crate::processo::comando("node");
    cmd.arg("--version");
    if let Some(p) = crate::user_env::sidecar_path() {
        cmd.env("PATH", p);
    }
    super::saida_com_prazo(cmd, super::PRAZO_VERSAO)
        .ok()
        .filter(|o| o.status.success())
        .is_some_and(|o| versao_do_node_serve(&String::from_utf8_lossy(&o.stdout)))
}

/// `SHVIA_RUNNERS_AUTO=0` (also `false`, `no`, `nao`, `off`) keeps the app from installing a missing runner:
/// a first install downloads about 110 MB (276 MB on disk, measured 06/10/2026), and a metered connection is the person's to protect. Updating an
/// installed runner is not covered by it (that is ADR-036, and it downloads when the app brought a newer one).
pub(crate) fn instalacao_desligada(valor: Option<&str>) -> bool {
    matches!(valor.map(|v| v.trim().to_lowercase()).as_deref(), Some("0" | "false" | "no" | "nao" | "não" | "off"))
}

/// The cases a notification is not repeated for: the same runner, the same app version, the same reason.
/// A failed install is tried again at every start (it may have been the network), but the person is told
/// once per app version — a sentence on every launch would teach them to ignore it.
const REGISTRO: &str = "runners-instalacao.json";

#[derive(Debug, Default, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Registro(std::collections::BTreeMap<String, (String, String)>);

impl Registro {
    pub(crate) fn ler(texto: &str) -> Self {
        serde_json::from_str(texto).unwrap_or_default()
    }

    pub(crate) fn escrever(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    /// Was this exact case already told for this runner on this app version?
    pub(crate) fn ja_avisou(&self, base: &str, app: &str, caso: &str) -> bool {
        self.0.get(base).is_some_and(|(a, c)| a == app && c == caso)
    }

    pub(crate) fn marcar(&mut self, base: &str, app: &str, caso: &str) {
        self.0.insert(base.to_string(), (app.to_string(), caso.to_string()));
    }

    pub(crate) fn esquecer(&mut self, base: &str) {
        self.0.remove(base);
    }
}

fn nome_do_motor(base: &str) -> &'static str {
    if base == "codex-runner" { "Codex" } else { "Claude Code" }
}

/// The words of a first install, per runner. `aviso` returns `None` when there is nothing to say.
pub(crate) fn texto_do_resultado(base: &str, r: &Resultado) -> Option<String> {
    let motor = nome_do_motor(base);
    let ultima = |erro: &str| erro.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
    match r {
        Resultado::Atualizado { de, para } => Some(format!("Runner do {motor} atualizado ({de} → {para}), junto com o app.")),
        Resultado::Falhou { de, para, erro } => Some(format!(
            "Não consegui atualizar o runner do {motor} ({de} → {para}). No Modo Code, use \"Instalar runner\". {}",
            ultima(erro),
        )),
        Resultado::Instalado { .. } => Some(format!("Motor do {motor} instalado junto com o app. Já dá para usar no Modo Code.")),
        Resultado::FalhouAoInstalar { erro, .. } => Some(format!(
            "Não consegui instalar o motor do {motor}, que o Modo Code usa. Em Configurações → Motor Claude Code há o botão para tentar de novo. {}",
            ultima(erro),
        )),
        Resultado::FaltaNode { .. } => Some(format!(
            "O motor do {motor} (Modo Code) precisa do Node.js 18 ou mais novo, e não achei no seu computador. Instale o Node e reabra o ShvIA: ele instala o motor sozinho.",
        )),
        Resultado::Nada(_) | Resultado::Desligado | Resultado::OcupadoPeloBotao => None,
    }
}

/// The case a notification belongs to, for `Registro`: only the reasons that repeat at every start.
fn caso_repetivel(r: &Resultado) -> Option<&'static str> {
    match r {
        Resultado::FalhouAoInstalar { .. } => Some("falhou_ao_instalar"),
        Resultado::FaltaNode { .. } => Some("falta_node"),
        _ => None,
    }
}

/// Everything a start needs from the machine, so the whole pass is one function a test can drive: where the
/// bundle is, where the notice register lives, the environment, what the app already finds of a runner, whether
/// Node is usable, how to run the installer, and how to tell the person.
pub(crate) struct Maquina<'a> {
    pub(crate) recursos: &'a Path,
    pub(crate) registro: Option<PathBuf>,
    pub(crate) windows: bool,
    pub(crate) versao_do_app: &'a str,
    pub(crate) env: &'a dyn Fn(&str) -> Option<String>,
    pub(crate) achado: &'a dyn Fn(&str) -> bool,
    pub(crate) node_serve: &'a dyn Fn() -> bool,
    pub(crate) instalar: &'a dyn Fn(&str) -> Result<String, String>,
    pub(crate) avisar: &'a dyn Fn(&str),
}

/// One pass over the runners the app ships. Returns what happened to each, for the log and for tests.
pub(crate) fn passar_pelos_runners(m: &Maquina) -> Vec<(&'static str, Resultado)> {
    let mut registro = m
        .registro
        .as_ref()
        .and_then(|a| std::fs::read_to_string(a).ok())
        .map(|t| Registro::ler(&t))
        .unwrap_or_default();
    let mut saida = Vec::new();
    // `node --version` costs a process: asked at most once per pass, and only when a runner is missing.
    let node = std::cell::OnceCell::new();
    for base in RUNNERS {
        let (Some(empacotada), Some(instalada)) = (pasta_empacotada(m.recursos, base), pasta_instalada(base, m.windows, m.env)) else {
            continue;
        };
        let estado = match ((m.achado)(base), instalada.is_dir()) {
            (true, true) => Estado::Instalado,
            (false, _) => Estado::Ausente,
            (true, false) => Estado::EmOutroLugar,
        };
        let motor = nome_do_motor(base);
        // A retry of an install that already failed on this app version is silent until it ends: a "starting"
        // sentence at every launch of an offline machine is the sentence the person learns to ignore.
        let retentativa = registro.ja_avisou(base, m.versao_do_app, "falhou_ao_instalar");
        let resultado = atualizar_se_preciso(
            base,
            &empacotada,
            &instalada,
            estado,
            &|| {
                if instalacao_desligada((m.env)("SHVIA_RUNNERS_AUTO").as_deref()) {
                    PrimeiraInstalacao::Desligada
                } else if *node.get_or_init(|| (m.node_serve)()) {
                    PrimeiraInstalacao::Permitida
                } else {
                    PrimeiraInstalacao::SemNode
                }
            },
            &|| {
                if !retentativa {
                    (m.avisar)(&format!(
                        "Instalando o motor do {motor} para o Modo Code. Baixa uns 110 MB (ocupa uns 280 MB no disco) e pode levar alguns minutos; aviso quando terminar."
                    ))
                }
            },
            &|| (m.instalar)(base),
        );
        // A reason that comes back at every start is told once per app version; anything else (an
        // install that worked, an update) is told when it happens.
        let aviso = match caso_repetivel(&resultado) {
            Some(caso) if registro.ja_avisou(base, m.versao_do_app, caso) => None,
            Some(caso) => {
                registro.marcar(base, m.versao_do_app, caso);
                texto_do_resultado(base, &resultado)
            }
            None => {
                if matches!(resultado, Resultado::Instalado { .. } | Resultado::Atualizado { .. }) {
                    registro.esquecer(base);
                }
                texto_do_resultado(base, &resultado)
            }
        };
        if let Some(corpo) = aviso {
            (m.avisar)(&corpo);
        }
        saida.push((base, resultado));
    }
    if let Some(a) = &m.registro {
        if let Some(dir) = a.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(a, registro.escrever());
    }
    saida
}

/// At every start, in the background: installs the runner the app brought when the machine has none
/// (1.13.0, ADR-040), brings each installed one up to the bundled one (1.7.1, ADR-036), and says so in a
/// native notification. Never blocks the window and never fails the start.
pub(crate) fn na_abertura(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let Ok(recursos) = app.path().resource_dir() else {
            return;
        };
        let avisar = |corpo: &str| {
            let _ = app.notification().builder().title("ShvIA").body(corpo.replace(['<', '>'], " ")).show();
        };
        let resultados = passar_pelos_runners(&Maquina {
            recursos: &recursos,
            registro: app.path().app_config_dir().ok().map(|d| d.join(REGISTRO)),
            windows: cfg!(windows),
            versao_do_app: env!("CARGO_PKG_VERSION"),
            env: &|k| std::env::var(k).ok(),
            achado: &|base| super::motores::resolve_runner(base).is_some(),
            node_serve: &node_serve,
            instalar: &|base| super::motores::instalar_runner(&app, base).map_err(|(_, msg)| msg),
            avisar: &avisar,
        });
        for (base, resultado) in resultados {
            eprintln!("ShvIA: runner {base}: {resultado:?}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The install locks are process-wide (one per runner), and several tests here take them. Run in parallel, a
    /// test would find the lock held by another and read `OcupadoPeloBotao` where it expected an install — a
    /// failure that depends on timing. Every test that touches the locks holds this for its whole life.
    static EM_SERIE: Mutex<()> = Mutex::new(());

    fn em_serie() -> MutexGuard<'static, ()> {
        EM_SERIE.lock().unwrap_or_else(|p| p.into_inner())
    }

    #[test]
    fn a_regra_nas_situacoes() {
        // 🔴 06/10/2026, the owner's second machine: updated to the latest app and the Code engine had no
        // runner at all, because this row used to say "nothing". It installs now.
        assert_eq!(
            decidir(Estado::Ausente, None, "1.5.9", false),
            Decisao::Instalar { para: "1.5.9".into() },
        );
        assert_eq!(
            decidir(Estado::EmOutroLugar, Some("1.0.0"), "1.5.9", false),
            Decisao::ForaDoPadrao,
            "a runner someone put elsewhere is not installed over",
        );
        assert_eq!(decidir(Estado::Instalado, Some("1.5.9"), "1.5.9", true), Decisao::EmDia);
        // The owner's machine on 25/09/2026.
        assert_eq!(
            decidir(Estado::Instalado, Some("1.4.34"), "1.5.9", false),
            Decisao::Atualizar { de: "1.4.34".into(), para: "1.5.9".into() },
        );
        assert_eq!(
            decidir(Estado::Instalado, Some("1.5.10"), "1.5.9", false),
            Decisao::MaisNovo { instalada: "1.5.10".into(), empacotada: "1.5.9".into() },
            "a runner installed by hand from a newer checkout is not downgraded",
        );
    }

    /// 🔴 Equal numbers are not equal runners: the Codex runner changed 8 times with 7 bumps.
    #[test]
    fn mesma_versao_com_arquivos_diferentes_atualiza() {
        assert_eq!(
            decidir(Estado::Instalado, Some("1.5.9"), "1.5.9", false),
            Decisao::Atualizar { de: "1.5.9".into(), para: "1.5.9".into() },
        );
    }

    #[test]
    fn versao_ilegivel_conta_como_mais_velha() {
        assert_eq!(
            decidir(Estado::Instalado, None, "1.7.1", true),
            Decisao::Atualizar { de: "?".into(), para: "1.7.1".into() },
        );
    }

    /// Numbers, not text: as strings, "1.4.34" sorts after "1.10.0".
    #[test]
    fn versoes_comparam_como_numeros() {
        assert_eq!(comparar("1.4.34", "1.10.0"), Ordering::Less);
        assert_eq!(comparar("1.6.58", "1.6.58"), Ordering::Equal);
        assert_eq!(comparar("1.6", "1.6.0"), Ordering::Equal);
        assert_eq!(comparar("2.0.0", "1.99.99"), Ordering::Greater);
    }

    #[test]
    fn as_pastas_que_os_instaladores_usam() {
        let env = |pares: &'static [(&'static str, &'static str)]| {
            move |k: &str| pares.iter().find(|(c, _)| *c == k).map(|(_, v)| v.to_string())
        };
        assert_eq!(
            pasta_instalada("codex-runner", false, &env(&[("HOME", "/home/x")])),
            Some(PathBuf::from("/home/x/.local/share/shvia-codex-runner")),
        );
        assert_eq!(
            pasta_instalada("codex-runner", false, &env(&[("HOME", "/home/x"), ("XDG_DATA_HOME", "/dados")])),
            Some(PathBuf::from("/dados/shvia-codex-runner")),
        );
        assert_eq!(
            pasta_instalada("claude-runner", true, &env(&[("LOCALAPPDATA", "C:/Users/x/AppData/Local")])),
            Some(PathBuf::from("C:/Users/x/AppData/Local/shvia-claude-runner")),
        );
        assert_eq!(pasta_instalada("claude-runner", true, &env(&[])), None);
    }

    /// A temporary pair of folders shaped like the bundle and the install.
    struct Pastas {
        raiz: PathBuf,
        _serie: MutexGuard<'static, ()>,
    }

    impl Pastas {
        fn novas(nome: &str) -> Self {
            let serie = em_serie();
            let raiz = std::env::temp_dir().join(format!("shvia-atualizacao-{nome}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&raiz);
            for d in ["app/codex-runner", "app/claude-runner", "casa/shvia-codex-runner", "casa/claude-runner"] {
                std::fs::create_dir_all(raiz.join(d)).unwrap();
            }
            Self { raiz, _serie: serie }
        }
        fn escrever(&self, rel: &str, conteudo: &str) {
            std::fs::write(self.raiz.join(rel), conteudo).unwrap();
        }
        fn empacotada(&self) -> PathBuf {
            self.raiz.join("app/codex-runner")
        }
        fn instalada(&self) -> PathBuf {
            self.raiz.join("casa/shvia-codex-runner")
        }
    }

    impl Drop for Pastas {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.raiz);
        }
    }

    fn runner_igual(p: &Pastas) {
        for lado in ["app/codex-runner", "casa/shvia-codex-runner"] {
            p.escrever(&format!("{lado}/codex-runner.mjs"), "runner");
            p.escrever(&format!("{lado}/package.json"), r#"{"version":"1.5.9"}"#);
        }
        p.escrever("app/claude-runner/politica.mjs", "politica");
        p.escrever("casa/claude-runner/politica.mjs", "politica");
    }

    #[test]
    fn arquivos_iguais_compara_so_o_que_existe_dos_dois_lados() {
        let p = Pastas::novas("iguais");
        runner_igual(&p);
        // Bundled but never copied by the installer: must not make every start look outdated.
        p.escrever("app/codex-runner/ciclo.test.mjs", "teste");
        p.escrever("app/codex-runner/README.md", "leia");
        p.escrever("app/codex-runner/novo.mjs", "módulo que a instalação velha não tinha");
        assert!(arquivos_iguais(&p.empacotada(), &p.instalada(), "codex-runner"));

        p.escrever("casa/shvia-codex-runner/codex-runner.mjs", "runner antigo");
        assert!(!arquivos_iguais(&p.empacotada(), &p.instalada(), "codex-runner"));
    }

    /// 🔴 The Codex engine runs the policy copy its installer left next to it.
    #[test]
    fn a_politica_do_codex_entra_na_comparacao() {
        let p = Pastas::novas("politica");
        runner_igual(&p);
        p.escrever("casa/claude-runner/politica.mjs", "politica velha");
        assert!(!arquivos_iguais(&p.empacotada(), &p.instalada(), "codex-runner"));
    }

    /// The three callbacks a test does not care about.
    fn permitida() -> PrimeiraInstalacao {
        PrimeiraInstalacao::Permitida
    }

    #[test]
    fn atualiza_quem_ficou_para_tras() {
        let p = Pastas::novas("fluxo");
        runner_igual(&p);
        p.escrever("casa/shvia-codex-runner/package.json", r#"{"version":"1.4.34"}"#);
        let chamadas = std::cell::Cell::new(0);
        let instalar = || {
            chamadas.set(chamadas.get() + 1);
            Ok::<_, String>("✓".into())
        };

        assert_eq!(
            atualizar_se_preciso("codex-runner", &p.empacotada(), &p.instalada(), Estado::Instalado, &permitida, &|| panic!("an update is not a first install"), &instalar),
            Resultado::Atualizado { de: "1.4.34".into(), para: "1.5.9".into() },
        );
        assert_eq!(chamadas.get(), 1);
    }

    /// 🔴 The row that changed: a missing runner is installed, and the person is told it started.
    #[test]
    fn instala_o_que_falta_e_avisa_que_comecou() {
        let p = Pastas::novas("primeira");
        runner_igual(&p);
        let chamadas = std::cell::Cell::new(0);
        let comecou = std::cell::Cell::new(0);
        let r = atualizar_se_preciso(
            "codex-runner",
            &p.empacotada(),
            &p.instalada(),
            Estado::Ausente,
            &permitida,
            &|| comecou.set(comecou.get() + 1),
            &|| {
                chamadas.set(chamadas.get() + 1);
                Ok::<_, String>("✓".into())
            },
        );
        assert_eq!(r, Resultado::Instalado { para: "1.5.9".into() });
        assert_eq!((chamadas.get(), comecou.get()), (1, 1), "one install, announced once");
    }

    /// Switched off, or no Node: the installer is never run, and the reason comes back as a word.
    #[test]
    fn sem_node_ou_desligado_nao_roda_o_instalador() {
        let p = Pastas::novas("barrada");
        runner_igual(&p);
        for (primeira, esperado) in [
            (PrimeiraInstalacao::SemNode, Resultado::FaltaNode { para: "1.5.9".into() }),
            (PrimeiraInstalacao::Desligada, Resultado::Desligado),
        ] {
            let r = atualizar_se_preciso(
                "codex-runner",
                &p.empacotada(),
                &p.instalada(),
                Estado::Ausente,
                &|| primeira,
                &|| panic!("nothing is announced when nothing starts"),
                &|| panic!("the installer must not run"),
            );
            assert_eq!(r, esperado);
        }
    }

    /// The machine is asked about Node only when a runner is MISSING: an ordinary start costs no process.
    #[test]
    fn so_pergunta_pelo_node_quando_falta_runner() {
        let p = Pastas::novas("pergunta");
        runner_igual(&p);
        for estado in [Estado::Instalado, Estado::EmOutroLugar] {
            let r = atualizar_se_preciso(
                "codex-runner",
                &p.empacotada(),
                &p.instalada(),
                estado,
                &|| panic!("nothing is missing: nothing to ask"),
                &|| panic!("nothing starts"),
                &|| panic!("nothing installs"),
            );
            assert!(matches!(r, Resultado::Nada(_)), "{estado:?} → {r:?}");
        }
    }

    #[test]
    fn um_runner_em_outro_lugar_fica_como_esta() {
        let p = Pastas::novas("outro");
        runner_igual(&p);
        let r = atualizar_se_preciso(
            "codex-runner", &p.empacotada(), &p.instalada(), Estado::EmOutroLugar,
            &permitida, &|| panic!("no"), &|| panic!("a developer's own runner is not installed over"),
        );
        assert_eq!(r, Resultado::Nada(Decisao::ForaDoPadrao));
    }

    #[test]
    fn um_build_sem_a_fonte_nao_instala_nada() {
        let p = Pastas::novas("semfonte");
        // no bundled package.json at all
        let r = atualizar_se_preciso(
            "codex-runner", &p.empacotada(), &p.instalada(), Estado::Ausente,
            &permitida, &|| panic!("no"), &|| panic!("nothing to install from"),
        );
        assert_eq!(r, Resultado::Nada(Decisao::SemFonte));
    }

    #[test]
    fn a_falha_do_instalador_volta_com_o_erro() {
        let p = Pastas::novas("falha");
        runner_igual(&p);
        p.escrever("casa/shvia-codex-runner/package.json", r#"{"version":"1.4.34"}"#);
        let r = atualizar_se_preciso("codex-runner", &p.empacotada(), &p.instalada(), Estado::Instalado, &permitida, &|| panic!("no"), &|| {
            Err("erro: Node 18+ não encontrado no PATH.".to_string())
        });
        assert_eq!(
            r,
            Resultado::Falhou {
                de: "1.4.34".into(),
                para: "1.5.9".into(),
                erro: "erro: Node 18+ não encontrado no PATH.".into(),
            },
        );
    }

    #[test]
    fn a_falha_de_uma_primeira_instalacao_volta_com_o_erro() {
        let p = Pastas::novas("falha1");
        runner_igual(&p);
        let r = atualizar_se_preciso("codex-runner", &p.empacotada(), &p.instalada(), Estado::Ausente, &permitida, &|| {}, &|| {
            Err("npm ERR! network".to_string())
        });
        assert_eq!(r, Resultado::FalhouAoInstalar { para: "1.5.9".into(), erro: "npm ERR! network".into() });
    }

    /// The button and the start never install the same runner at once.
    ///
    /// Holds the Claude lock for the whole test; `em_serie` (through `Pastas`) keeps the other tests that
    /// take the locks from running at the same time.
    #[test]
    fn com_o_botao_instalando_a_abertura_nao_entra() {
        let p = Pastas::novas("ocupado");
        runner_igual(&p);
        p.escrever("casa/shvia-codex-runner/package.json", r#"{"version":"1.4.34"}"#);
        let _botao = tentar_instalar("claude-runner").expect("free at the start of the test");
        assert!(instalando("claude-runner"));
        // An update AND a first install: both wait for the button's install to end.
        for estado in [Estado::Instalado, Estado::Ausente] {
            let r = atualizar_se_preciso("claude-runner", &p.empacotada(), &p.instalada(), estado, &permitida, &|| panic!("no"), &|| {
                panic!("the start must not run the installer while the button holds the lock")
            });
            assert_eq!(r, Resultado::OcupadoPeloBotao, "{estado:?}");
        }
    }

    #[test]
    fn o_node_serve_do_dezoito_em_diante() {
        assert!(versao_do_node_serve("v24.15.0\n"));
        assert!(versao_do_node_serve("v18.0.0"));
        assert!(versao_do_node_serve("20.1.0"));
        assert!(!versao_do_node_serve("v16.20.2"));
        assert!(!versao_do_node_serve("v8.17.0"));
        assert!(!versao_do_node_serve(""));
        assert!(!versao_do_node_serve("bash: node: command not found"));
    }

    #[test]
    fn so_um_nao_explicito_desliga_a_instalacao() {
        for v in ["0", "false", "FALSE", " no ", "nao", "não", "off"] {
            assert!(instalacao_desligada(Some(v)), "{v:?} switches it off");
        }
        for v in ["1", "true", "", "sim", "talvez"] {
            assert!(!instalacao_desligada(Some(v)), "{v:?} leaves it on");
        }
        assert!(!instalacao_desligada(None), "unset is on: that is the point of the change");
    }

    /// A reason that returns at every start is told once per app version, per runner.
    #[test]
    fn o_registro_avisa_uma_vez_por_versao_do_app() {
        let mut r = Registro::default();
        assert!(!r.ja_avisou("claude-runner", "1.13.0", "falta_node"));
        r.marcar("claude-runner", "1.13.0", "falta_node");
        assert!(r.ja_avisou("claude-runner", "1.13.0", "falta_node"));
        assert!(!r.ja_avisou("claude-runner", "1.13.1", "falta_node"), "a new app version tells again");
        assert!(!r.ja_avisou("claude-runner", "1.13.0", "falhou_ao_instalar"), "another reason is another sentence");
        assert!(!r.ja_avisou("codex-runner", "1.13.0", "falta_node"), "per runner");

        let de_volta = Registro::ler(&r.escrever());
        assert_eq!(de_volta, r, "what is written is what is read");
        assert_eq!(Registro::ler("lixo { não é json"), Registro::default(), "a damaged file is an empty one, never a crash");
        r.esquecer("claude-runner");
        assert!(!r.ja_avisou("claude-runner", "1.13.0", "falta_node"));
    }

    #[test]
    fn so_o_que_se_repete_a_cada_abertura_entra_no_registro() {
        assert_eq!(caso_repetivel(&Resultado::FaltaNode { para: "1".into() }), Some("falta_node"));
        assert_eq!(caso_repetivel(&Resultado::FalhouAoInstalar { para: "1".into(), erro: "x".into() }), Some("falhou_ao_instalar"));
        assert_eq!(caso_repetivel(&Resultado::Instalado { para: "1".into() }), None);
        assert_eq!(caso_repetivel(&Resultado::Atualizado { de: "1".into(), para: "2".into() }), None);
        assert_eq!(caso_repetivel(&Resultado::Desligado), None);
    }

    #[test]
    fn as_frases_dizem_o_motor_e_o_que_fazer() {
        let node = texto_do_resultado("claude-runner", &Resultado::FaltaNode { para: "1.13.0".into() }).unwrap();
        assert!(node.contains("Claude Code") && node.contains("Node.js 18"), "{node}");
        assert!(node.contains("reabra"), "it says what to do next: {node}");

        let falha = texto_do_resultado(
            "codex-runner",
            &Resultado::FalhouAoInstalar { para: "1.13.0".into(), erro: "npm ERR! x\nnpm ERR! network timeout\n\n".into() },
        )
        .unwrap();
        assert!(falha.contains("Codex") && falha.contains("network timeout"), "{falha}");
        assert!(falha.contains("Configurações"), "it points at the button: {falha}");

        let ok = texto_do_resultado("claude-runner", &Resultado::Instalado { para: "1.13.0".into() }).unwrap();
        assert!(ok.contains("instalado"), "{ok}");

        for calado in [Resultado::Desligado, Resultado::OcupadoPeloBotao, Resultado::Nada(Decisao::EmDia)] {
            assert_eq!(texto_do_resultado("claude-runner", &calado), None, "{calado:?}");
        }
    }

    // ── the whole pass, with the machine passed in ───────────────────────────────────────────────────────────

    use std::cell::{Cell, RefCell};

    /// A machine in a temp folder: a bundle that carries both runners at 1.13.0, a home, a notice register.
    struct Cenario {
        raiz: PathBuf,
        _serie: MutexGuard<'static, ()>,
        node: Cell<bool>,
        falha: Cell<bool>,
        achado_fora: Cell<bool>,
        sondas_de_node: Cell<u32>,
        versao_do_app: RefCell<String>,
        env: RefCell<Vec<(String, String)>>,
        avisos: RefCell<Vec<String>>,
        instalacoes: RefCell<Vec<String>>,
    }

    impl Cenario {
        fn nova(nome: &str) -> Self {
            let serie = em_serie();
            let raiz = std::env::temp_dir().join(format!("shvia-abertura-{nome}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&raiz);
            for base in RUNNERS {
                let d = raiz.join("recursos/_up_").join(base);
                std::fs::create_dir_all(&d).unwrap();
                std::fs::write(d.join("package.json"), r#"{"version":"1.13.0"}"#).unwrap();
            }
            std::fs::create_dir_all(raiz.join("casa")).unwrap();
            Self {
                raiz,
                _serie: serie,
                node: Cell::new(true),
                falha: Cell::new(false),
                achado_fora: Cell::new(false),
                sondas_de_node: Cell::new(0),
                versao_do_app: RefCell::new("1.13.0".into()),
                env: RefCell::new(vec![]),
                avisos: RefCell::new(vec![]),
                instalacoes: RefCell::new(vec![]),
            }
        }

        fn instalada(&self, base: &str) -> PathBuf {
            self.raiz.join("casa/.local/share").join(format!("shvia-{base}"))
        }

        fn registro(&self) -> PathBuf {
            self.raiz.join("config").join(REGISTRO)
        }

        fn rodar(&self) -> Vec<(&'static str, Resultado)> {
            let recursos = self.raiz.join("recursos");
            let versao = self.versao_do_app.borrow().clone();
            passar_pelos_runners(&Maquina {
                recursos: &recursos,
                registro: Some(self.registro()),
                windows: false,
                versao_do_app: &versao,
                env: &|k| {
                    if k == "HOME" {
                        return Some(self.raiz.join("casa").to_string_lossy().to_string());
                    }
                    self.env.borrow().iter().find(|(c, _)| c == k).map(|(_, v)| v.clone())
                },
                achado: &|base| self.instalada(base).is_dir() || self.achado_fora.get(),
                node_serve: &|| {
                    self.sondas_de_node.set(self.sondas_de_node.get() + 1);
                    self.node.get()
                },
                instalar: &|base| {
                    self.instalacoes.borrow_mut().push(base.to_string());
                    if self.falha.get() {
                        return Err("npm ERR! code ENOTFOUND\nnpm ERR! network request failed\n".to_string());
                    }
                    let destino = self.instalada(base);
                    std::fs::create_dir_all(&destino).unwrap();
                    std::fs::copy(self.raiz.join("recursos/_up_").join(base).join("package.json"), destino.join("package.json")).unwrap();
                    Ok("✓".to_string())
                },
                avisar: &|texto| self.avisos.borrow_mut().push(texto.to_string()),
            })
        }

        fn avisos(&self) -> Vec<String> {
            std::mem::take(&mut *self.avisos.borrow_mut())
        }
    }

    impl Drop for Cenario {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.raiz);
        }
    }

    /// 🔴 The case that started this: a machine that updated the app and has neither runner. Both come, each
    /// announced when it starts and when it ends, and the next start has nothing to say.
    #[test]
    fn uma_maquina_sem_os_motores_recebe_os_dois_e_o_proximo_inicio_nao_diz_nada() {
        let c = Cenario::nova("nova");

        let r = c.rodar();
        assert_eq!(
            r,
            vec![
                ("claude-runner", Resultado::Instalado { para: "1.13.0".into() }),
                ("codex-runner", Resultado::Instalado { para: "1.13.0".into() }),
            ],
        );
        assert_eq!(*c.instalacoes.borrow(), vec!["claude-runner", "codex-runner"]);
        let avisos = c.avisos();
        assert_eq!(avisos.len(), 4, "start and end of each: {avisos:?}");
        assert!(avisos[0].starts_with("Instalando o motor do Claude Code") && avisos[0].contains("110 MB"), "{avisos:?}");
        assert!(avisos[1].contains("Claude Code instalado"), "{avisos:?}");
        assert!(avisos[2].contains("Codex") && avisos[3].contains("Codex instalado"), "{avisos:?}");

        let de_novo = c.rodar();
        assert!(de_novo.iter().all(|(_, r)| *r == Resultado::Nada(Decisao::EmDia)), "{de_novo:?}");
        assert_eq!(c.instalacoes.borrow().len(), 2, "nothing is installed twice");
        assert!(c.avisos().is_empty(), "an ordinary start says nothing");
        assert_eq!(c.sondas_de_node.get(), 1, "Node was asked about once, for the first install, not at every start");
    }

    #[test]
    fn sem_node_diz_uma_vez_por_versao_e_instala_quando_o_node_aparece() {
        let c = Cenario::nova("semnode");
        c.node.set(false);

        let r = c.rodar();
        assert!(r.iter().all(|(_, r)| matches!(r, Resultado::FaltaNode { .. })), "{r:?}");
        assert!(c.instalacoes.borrow().is_empty());
        assert_eq!(c.avisos().len(), 2, "one sentence per engine");

        c.rodar();
        assert!(c.avisos().is_empty(), "the same reason on the same app version is not told again");

        *c.versao_do_app.borrow_mut() = "1.13.1".into();
        c.rodar();
        assert_eq!(c.avisos().len(), 2, "a new app version tells again");

        c.node.set(true);
        let r = c.rodar();
        assert!(r.iter().all(|(_, r)| matches!(r, Resultado::Instalado { .. })), "{r:?}");
        let registro = Registro::ler(&std::fs::read_to_string(c.registro()).unwrap());
        assert!(!registro.ja_avisou("claude-runner", "1.13.1", "falta_node"), "the register forgets what was fixed");
    }

    #[test]
    fn a_falha_repetida_nao_repete_o_aviso_e_o_sucesso_depois_avisa() {
        let c = Cenario::nova("falha");
        c.falha.set(true);

        let r = c.rodar();
        assert!(r.iter().all(|(_, r)| matches!(r, Resultado::FalhouAoInstalar { .. })), "{r:?}");
        let avisos = c.avisos();
        assert_eq!(avisos.len(), 4, "start and failure of each: {avisos:?}");
        assert!(avisos[1].contains("Não consegui instalar") && avisos[1].contains("network request failed"), "{avisos:?}");

        c.rodar();
        assert_eq!(c.instalacoes.borrow().len(), 4, "it tries again at the next start: it may have been the network");
        assert!(c.avisos().is_empty(), "…without a word: not the 'starting' sentence, not the failure again");

        c.falha.set(false);
        c.rodar();
        let avisos = c.avisos();
        assert_eq!(avisos.len(), 2, "the connection came back: the retry stayed silent while it ran, the END is told: {avisos:?}");
        assert!(avisos.iter().all(|a| a.contains("instalado junto com o app")), "{avisos:?}");
    }

    #[test]
    fn desligado_nao_instala_nao_avisa_e_nem_pergunta_pelo_node() {
        let c = Cenario::nova("desligado");
        c.env.borrow_mut().push(("SHVIA_RUNNERS_AUTO".into(), "0".into()));

        let r = c.rodar();
        assert!(r.iter().all(|(_, r)| *r == Resultado::Desligado), "{r:?}");
        assert!(c.instalacoes.borrow().is_empty());
        assert!(c.avisos().is_empty(), "a person who switched it off is not nagged about it");
        assert_eq!(c.sondas_de_node.get(), 0);
    }

    #[test]
    fn um_runner_de_outro_lugar_nao_e_tocado() {
        let c = Cenario::nova("outro");
        c.achado_fora.set(true);

        let r = c.rodar();
        assert!(r.iter().all(|(_, r)| *r == Resultado::Nada(Decisao::ForaDoPadrao)), "{r:?}");
        assert!(c.instalacoes.borrow().is_empty() && c.avisos().is_empty());
        assert_eq!(c.sondas_de_node.get(), 0);
    }

    #[test]
    fn um_registro_danificado_nao_derruba_a_abertura() {
        let c = Cenario::nova("danificado");
        std::fs::create_dir_all(c.registro().parent().unwrap()).unwrap();
        std::fs::write(c.registro(), "{ isto não é json").unwrap();

        let r = c.rodar();
        assert!(r.iter().all(|(_, r)| matches!(r, Resultado::Instalado { .. })), "{r:?}");
        assert!(serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(c.registro()).unwrap()).is_ok(), "rewritten as valid JSON");
    }

    #[test]
    fn um_build_sem_os_motores_nao_faz_nada() {
        let c = Cenario::nova("vazio");
        std::fs::remove_dir_all(c.raiz.join("recursos/_up_")).unwrap();

        assert!(c.rodar().is_empty());
        assert!(c.instalacoes.borrow().is_empty() && c.avisos().is_empty());
    }
}
