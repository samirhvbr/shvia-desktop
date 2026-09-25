//! The installed runners follow the app (1.7.1).
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
//! - **not installed** → nothing. The first install stays the "Instalar runner" button's: it is
//!   the user's consent, and an engine nobody set up is not the app's to install;
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

#[derive(Debug, PartialEq)]
pub(crate) enum Decisao {
    NaoInstalado,
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
pub(crate) fn decidir(instalado: bool, versao_instalada: Option<&str>, empacotada: &str, arquivos_iguais: bool) -> Decisao {
    if !instalado {
        return Decisao::NaoInstalado;
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

/// What happened to one runner at a start, for the notification and the log.
#[derive(Debug, PartialEq)]
pub(crate) enum Resultado {
    Nada(Decisao),
    Atualizado { de: String, para: String },
    Falhou { de: String, para: String, erro: String },
    OcupadoPeloBotao,
}

/// One runner, with the machine passed in: `instalado` is whether the app finds the runner and its
/// folder exists; `instalar` runs the bundled installer. Holds the install lock while it runs.
pub(crate) fn atualizar_se_preciso(
    base: &str,
    empacotada: &Path,
    instalada: &Path,
    instalado: bool,
    instalar: &dyn Fn() -> Result<String, String>,
) -> Resultado {
    let Some(versao_empacotada) = versao_em(empacotada) else {
        return Resultado::Nada(Decisao::NaoInstalado);
    };
    let decisao = decidir(
        instalado,
        versao_em(instalada).as_deref(),
        &versao_empacotada,
        arquivos_iguais(empacotada, instalada, base),
    );
    let Decisao::Atualizar { de, para } = decisao else {
        return Resultado::Nada(decisao);
    };
    let Some(_trava) = tentar_instalar(base) else {
        return Resultado::OcupadoPeloBotao;
    };
    match instalar() {
        Ok(_) => Resultado::Atualizado { de, para },
        Err(erro) => Resultado::Falhou { de, para, erro },
    }
}

fn nome_do_motor(base: &str) -> &'static str {
    if base == "codex-runner" { "Codex" } else { "Claude Code" }
}

/// At every start, in the background: brings each installed runner up to the one the app shipped,
/// and says so in a native notification. Never blocks the window and never fails the start.
pub(crate) fn na_abertura(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let Ok(recursos) = app.path().resource_dir() else {
            return;
        };
        let env = |k: &str| std::env::var(k).ok();
        for base in RUNNERS {
            let (Some(empacotada), Some(instalada)) =
                (pasta_empacotada(&recursos, base), pasta_instalada(base, cfg!(windows), &env))
            else {
                continue;
            };
            let instalado = instalada.is_dir() && super::motores::resolve_runner(base).is_some();
            let resultado = atualizar_se_preciso(base, &empacotada, &instalada, instalado, &|| {
                super::motores::instalar_runner(&app, base).map_err(|(_, msg)| msg)
            });
            let motor = nome_do_motor(base);
            let aviso = match &resultado {
                Resultado::Atualizado { de, para } => {
                    Some(format!("Runner do {motor} atualizado ({de} → {para}), junto com o app."))
                }
                Resultado::Falhou { de, para, erro } => Some(format!(
                    "Não consegui atualizar o runner do {motor} ({de} → {para}). No Modo Code, use \"Instalar runner\". {}",
                    erro.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim(),
                )),
                _ => None,
            };
            eprintln!("ShvIA: runner {base}: {resultado:?}");
            if let Some(corpo) = aviso {
                let _ = app.notification().builder().title("ShvIA").body(corpo.replace(['<', '>'], " ")).show();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_regra_nas_quatro_situacoes() {
        assert_eq!(decidir(false, Some("1.0.0"), "1.5.9", false), Decisao::NaoInstalado);
        assert_eq!(decidir(true, Some("1.5.9"), "1.5.9", true), Decisao::EmDia);
        // The owner's machine on 25/09/2026.
        assert_eq!(
            decidir(true, Some("1.4.34"), "1.5.9", false),
            Decisao::Atualizar { de: "1.4.34".into(), para: "1.5.9".into() },
        );
        assert_eq!(
            decidir(true, Some("1.5.10"), "1.5.9", false),
            Decisao::MaisNovo { instalada: "1.5.10".into(), empacotada: "1.5.9".into() },
            "a runner installed by hand from a newer checkout is not downgraded",
        );
    }

    /// 🔴 Equal numbers are not equal runners: the Codex runner changed 8 times with 7 bumps.
    #[test]
    fn mesma_versao_com_arquivos_diferentes_atualiza() {
        assert_eq!(
            decidir(true, Some("1.5.9"), "1.5.9", false),
            Decisao::Atualizar { de: "1.5.9".into(), para: "1.5.9".into() },
        );
    }

    #[test]
    fn versao_ilegivel_conta_como_mais_velha() {
        assert_eq!(
            decidir(true, None, "1.7.1", true),
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
    }

    impl Pastas {
        fn novas(nome: &str) -> Self {
            let raiz = std::env::temp_dir().join(format!("shvia-atualizacao-{nome}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&raiz);
            for d in ["app/codex-runner", "app/claude-runner", "casa/shvia-codex-runner", "casa/claude-runner"] {
                std::fs::create_dir_all(raiz.join(d)).unwrap();
            }
            Self { raiz }
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

    #[test]
    fn atualiza_quem_ficou_para_tras_e_so_quem_esta_instalado() {
        let p = Pastas::novas("fluxo");
        runner_igual(&p);
        p.escrever("casa/shvia-codex-runner/package.json", r#"{"version":"1.4.34"}"#);
        let chamadas = std::cell::Cell::new(0);
        let instalar = || {
            chamadas.set(chamadas.get() + 1);
            Ok::<_, String>("✓".into())
        };

        assert_eq!(
            atualizar_se_preciso("codex-runner", &p.empacotada(), &p.instalada(), false, &instalar),
            Resultado::Nada(Decisao::NaoInstalado),
        );
        assert_eq!(chamadas.get(), 0, "an engine nobody installed is not installed at start");

        assert_eq!(
            atualizar_se_preciso("codex-runner", &p.empacotada(), &p.instalada(), true, &instalar),
            Resultado::Atualizado { de: "1.4.34".into(), para: "1.5.9".into() },
        );
        assert_eq!(chamadas.get(), 1);
    }

    #[test]
    fn a_falha_do_instalador_volta_com_o_erro() {
        let p = Pastas::novas("falha");
        runner_igual(&p);
        p.escrever("casa/shvia-codex-runner/package.json", r#"{"version":"1.4.34"}"#);
        let r = atualizar_se_preciso("codex-runner", &p.empacotada(), &p.instalada(), true, &|| {
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

    /// The button and the start never install the same runner at once.
    ///
    /// ⚠️ On the CLAUDE lock on purpose: the locks are process-wide, the other tests here run the
    /// Codex one in parallel threads, and holding it would make them flaky. No other test takes
    /// the Claude lock.
    #[test]
    fn com_o_botao_instalando_a_abertura_nao_entra() {
        let p = Pastas::novas("ocupado");
        runner_igual(&p);
        p.escrever("casa/shvia-codex-runner/package.json", r#"{"version":"1.4.34"}"#);
        let _botao = tentar_instalar("claude-runner").expect("free at the start of the test");
        assert!(instalando("claude-runner"));
        let r = atualizar_se_preciso("claude-runner", &p.empacotada(), &p.instalada(), true, &|| {
            panic!("the start must not run the installer while the button holds the lock")
        });
        assert_eq!(r, Resultado::OcupadoPeloBotao);
    }
}
