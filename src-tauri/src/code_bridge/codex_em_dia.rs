//! The Codex CLI follows its own releases, once a day, from the app (1.44.0).
//!
//! ## Why
//!
//! The app updates itself and its runners (ADR-036, ADR-040); the `codex` the Codex runner starts is the person's own
//! install, and nothing updated it. Measured on the owner's Mac on 23/11/2026: Desktop 1.43.2, runner 1.7.0,
//! `codex-cli 0.163.0` from Homebrew, eight releases behind the 0.171.0 that npm and the Homebrew cask had. The owner
//! asked for it to be automatic.
//!
//! ## The rule
//!
//! At start, after the runners pass (so the runner asked is the one this app brought), at most once every
//! `INTERVALO`: `codex-runner --atualizar-codex` updates the `codex` it starts through the manager that installed it,
//! Homebrew or npm, and answers one JSON line (`codex-runner/atualizar.mjs`). The app only tells:
//!
//! - **updated** → a notification with both versions;
//! - **up to date**, or no `codex` → nothing;
//! - **failed** → a notification with the reason, once per newest version, so an offline machine is not told every day;
//! - **installed some other way** (a standalone install, a checkout) → nothing is run; once per newest version, the
//!   person is told a newer one exists.
//!
//! `SHVIA_CODEX_AUTO=0` (also `false`, `no`, `nao`, `off`) switches it off. A runner older than
//! `CODEX_ATUALIZA_DESDE` does not know the flag and would start in host mode with it, so it is never asked.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// The first `codex-runner` that answers `--atualizar-codex`.
pub(crate) const CODEX_ATUALIZA_DESDE: &str = "1.8.0";

/// At most one pass per this many seconds (20 h: a person who opens the app every morning gets one a day).
pub(crate) const INTERVALO: u64 = 20 * 60 * 60;

/// How long the runner gets: a Homebrew update fetches its own metadata first.
pub(crate) const PRAZO_ATUALIZACAO: Duration = Duration::from_secs(15 * 60);

pub(crate) const REGISTRO: &str = "codex-atualizacao.json";

/// What the runner answered.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct Linha {
    pub(crate) resultado: String,
    #[serde(default)]
    pub(crate) atual: Option<String>,
    #[serde(default)]
    pub(crate) ultima: Option<String>,
    #[serde(default)]
    pub(crate) novo: Option<String>,
    #[serde(default)]
    pub(crate) metodo: Option<String>,
    #[serde(default)]
    pub(crate) erro: Option<String>,
}

/// When the last pass ran, and the notices already given (`caso:versão`).
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct Registro {
    #[serde(default)]
    pub(crate) verificado_em: u64,
    #[serde(default)]
    pub(crate) avisos: Vec<String>,
}

/// Everything the pass touches, injected, so the tests drive it without a machine.
pub(crate) struct Maquina<'a> {
    pub(crate) registro: Option<PathBuf>,
    pub(crate) agora: u64,
    pub(crate) env: &'a dyn Fn(&str) -> Option<String>,
    /// The installed `codex-runner`'s version, or None without one.
    pub(crate) versao_do_runner: &'a dyn Fn() -> Option<String>,
    /// `codex-runner --atualizar-codex`: its stdout, or why it could not run.
    pub(crate) atualizar: &'a dyn Fn() -> Result<String, String>,
    pub(crate) avisar: &'a dyn Fn(&str),
}

/// The notification for a result, and the case it is remembered under when it must not repeat.
pub(crate) fn texto(l: &Linha) -> Option<(String, Option<String>)> {
    let atual = l.atual.as_deref().unwrap_or("?");
    match l.resultado.as_str() {
        "atualizado" => Some((format!("Codex atualizado: {atual} → {}.", l.novo.as_deref().unwrap_or("?")), None)),
        "falhou" => {
            let comando = match l.metodo.as_deref() {
                Some("brew-cask") => "brew upgrade --cask codex",
                Some("brew") => "brew upgrade codex",
                _ => "npm install -g @openai/codex@latest",
            };
            let porque = l.erro.as_deref().filter(|e| !e.is_empty()).unwrap_or("sem detalhe");
            Some((
                format!("Não consegui atualizar o Codex {atual} ({porque}). Para atualizar à mão: {comando}"),
                Some(format!("falhou:{}", l.ultima.as_deref().unwrap_or(atual))),
            ))
        }
        "sem_metodo" => {
            let ultima = l.ultima.as_deref().unwrap_or("?");
            Some((
                format!(
                    "Há Codex {ultima} (este computador tem {atual}). Ele não foi instalado pelo Homebrew nem pelo npm, então o ShvIA não o atualiza: atualize do jeito que instalou."
                ),
                Some(format!("sem_metodo:{ultima}")),
            ))
        }
        _ => None,
    }
}

/// One pass. Returns what the runner answered, or None when nothing was asked.
pub(crate) fn passar_pelo_codex(m: &Maquina) -> Option<Linha> {
    // The same spellings as `SHVIA_RUNNERS_AUTO`.
    if super::atualizacao::instalacao_desligada((m.env)("SHVIA_CODEX_AUTO").as_deref()) {
        return None;
    }
    let mut registro: Registro = m
        .registro
        .as_ref()
        .and_then(|a| std::fs::read_to_string(a).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    if m.agora.saturating_sub(registro.verificado_em) < INTERVALO {
        return None;
    }
    let runner = (m.versao_do_runner)()?;
    if super::atualizacao::comparar(&runner, CODEX_ATUALIZA_DESDE) == std::cmp::Ordering::Less {
        return None;
    }

    let linha = match (m.atualizar)() {
        Ok(saida) => saida.lines().rev().find_map(|l| serde_json::from_str::<Linha>(l.trim()).ok()),
        Err(porque) => Some(Linha {
            resultado: "falhou".into(),
            atual: None,
            ultima: None,
            novo: None,
            metodo: None,
            erro: Some(porque),
        }),
    };
    // A pass that ran counts, whatever it found: a machine that cannot update is asked again tomorrow, not at every start.
    registro.verificado_em = m.agora;
    if let Some(l) = &linha {
        if let Some((corpo, caso)) = texto(l) {
            match caso {
                Some(c) if registro.avisos.contains(&c) => {}
                Some(c) => {
                    registro.avisos.push(c);
                    (m.avisar)(&corpo);
                }
                None => {
                    registro.avisos.clear();
                    (m.avisar)(&corpo);
                }
            }
        }
    }
    if let Some(a) = &m.registro {
        if let Some(dir) = a.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(a, serde_json::to_string_pretty(&registro).unwrap_or_else(|_| "{}".into()));
    }
    linha
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Caso {
        dir: PathBuf,
        avisos: RefCell<Vec<String>>,
        perguntas: RefCell<u32>,
    }

    impl Drop for Caso {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    impl Caso {
        fn novo(nome: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("shvia-codex-em-dia-{nome}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Caso { dir, avisos: RefCell::default(), perguntas: RefCell::default() }
        }

        fn passar(&self, agora: u64, runner: Option<&str>, resposta: Result<&str, &str>, env: Option<&str>) -> Option<Linha> {
            let runner = runner.map(str::to_string);
            passar_pelo_codex(&Maquina {
                registro: Some(self.dir.join(REGISTRO)),
                agora,
                env: &|k| if k == "SHVIA_CODEX_AUTO" { env.map(str::to_string) } else { None },
                versao_do_runner: &|| runner.clone(),
                atualizar: &|| {
                    *self.perguntas.borrow_mut() += 1;
                    resposta.map(str::to_string).map_err(str::to_string)
                },
                avisar: &|c| self.avisos.borrow_mut().push(c.to_string()),
            })
        }
    }

    const DIA: u64 = 24 * 60 * 60;
    const ATUALIZADO: &str = r#"{"resultado":"atualizado","atual":"0.163.0","novo":"0.171.0","metodo":"brew-cask"}"#;
    const FALHOU: &str = r#"{"resultado":"falhou","atual":"0.163.0","ultima":"0.171.0","metodo":"npm","erro":"EACCES"}"#;

    #[test]
    fn an_update_is_told_with_both_versions_and_the_next_pass_waits_a_day() {
        let c = Caso::novo("atualizado");
        let l = c.passar(10 * DIA, Some("1.8.0"), Ok(ATUALIZADO), None).unwrap();
        assert_eq!(l.resultado, "atualizado");
        assert_eq!(*c.avisos.borrow(), vec!["Codex atualizado: 0.163.0 → 0.171.0.".to_string()]);
        // Two hours later nothing is asked; a day later it is.
        assert!(c.passar(10 * DIA + 2 * 3600, Some("1.8.0"), Ok(ATUALIZADO), None).is_none());
        assert_eq!(*c.perguntas.borrow(), 1);
        c.passar(11 * DIA, Some("1.8.0"), Ok(r#"{"resultado":"em_dia","atual":"0.171.0"}"#), None).unwrap();
        assert_eq!(*c.perguntas.borrow(), 2);
        assert_eq!(c.avisos.borrow().len(), 1, "up to date is silent");
    }

    #[test]
    fn a_failure_is_told_once_per_newest_version_with_the_command_to_run() {
        let c = Caso::novo("falhou");
        c.passar(10 * DIA, Some("1.8.0"), Ok(FALHOU), None);
        c.passar(11 * DIA, Some("1.8.0"), Ok(FALHOU), None);
        assert_eq!(c.avisos.borrow().len(), 1, "the same failure is not repeated every day");
        assert!(c.avisos.borrow()[0].contains("EACCES") && c.avisos.borrow()[0].contains("npm install -g @openai/codex@latest"));
        c.passar(12 * DIA, Some("1.8.0"), Ok(&FALHOU.replace("0.171.0", "0.172.0")), None);
        assert_eq!(c.avisos.borrow().len(), 2, "a newer version is a new case");
    }

    #[test]
    fn an_install_the_app_does_not_manage_is_only_told_about() {
        let c = Caso::novo("sem-metodo");
        c.passar(10 * DIA, Some("1.8.0"), Ok(r#"{"resultado":"sem_metodo","atual":"0.164.0","ultima":"0.171.0","metodo":"desconhecido"}"#), None);
        assert!(c.avisos.borrow()[0].starts_with("Há Codex 0.171.0 (este computador tem 0.164.0)"));
    }

    #[test]
    fn switched_off_an_old_runner_or_no_runner_asks_nothing() {
        let c = Caso::novo("desligado");
        assert!(c.passar(10 * DIA, Some("1.8.0"), Ok(ATUALIZADO), Some("0")).is_none());
        assert!(c.passar(10 * DIA, Some("1.7.0"), Ok(ATUALIZADO), None).is_none());
        assert!(c.passar(10 * DIA, None, Ok(ATUALIZADO), None).is_none());
        assert_eq!(*c.perguntas.borrow(), 0);
        assert!(c.avisos.borrow().is_empty());
    }

    #[test]
    fn a_runner_that_cannot_run_is_a_failure_told_once() {
        let c = Caso::novo("sem-runner");
        let l = c.passar(10 * DIA, Some("1.8.0"), Err("timed out"), None).unwrap();
        assert_eq!(l.resultado, "falhou");
        c.passar(11 * DIA, Some("1.8.0"), Err("timed out"), None);
        assert_eq!(c.avisos.borrow().len(), 1);
    }
}
