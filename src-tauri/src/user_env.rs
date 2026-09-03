//! PATH de verdade do usuário para os sidecars (ADR-029).
//!
//! Um app de GUI **não herda o PATH do shell**. No macOS quem inicia o `.app`
//! é o launchd, e o `PATH` que chega é o mínimo do sistema
//! (`/usr/bin:/bin:/usr/sbin:/sbin`) — `launchctl getenv PATH` costuma nem
//! existir. O `anna` roda as ferramentas com `sh -c`, herdando esse env: no Mac
//! do Samir isso deixou `npx`, `node`, `cargo`, `php` e `composer` (todos em
//! `/opt/homebrew/bin` ou `~/.cargo/bin`) INVISÍVEIS dentro do app, embora
//! funcionem no terminal.
//!
//! O sintoma não é "não encontrei": é o agente insistir. Caso real de 19/08/2026
//! (projeto KIDS, conversão de `.glb`): 100 voltas, 19 minutos, US$ 1,24 e 125
//! ferramentas repetindo `npx @gltf-transform/cli` que jamais poderia rodar —
//! e o erro ficava escondido porque os comandos redirecionavam para `/dev/null`.
//! Subir o teto de voltas (anna 0.11.0) só deu mais corda: a causa era o env.
//!
//! Solução: perguntar o PATH ao **shell de login do usuário**, uma vez por
//! processo, e passá-lo aos sidecars. É o mesmo desenho do `fix-path` do
//! Electron, pelo mesmo motivo.
//!
//! Confiança: o PATH sai dos dotfiles do PRÓPRIO usuário (`~/.zshrc` etc.) —
//! a mesma fronteira de confiança de abrir um terminal. **Nunca** vem da
//! página (que só manda url/model/effort/apiKey, e passa por allowlist).

#[cfg(not(windows))]
use std::sync::OnceLock;

/// PATH a passar para o sidecar, ou `None` para deixar o env como está.
///
/// No Windows o processo de GUI já recebe o PATH do usuário (registro/sessão),
/// então não há o que consertar — e um shell de login não existe para consultar.
#[cfg(windows)]
pub fn sidecar_path() -> Option<&'static str> {
    None
}

#[cfg(not(windows))]
pub fn sidecar_path() -> Option<&'static str> {
    static CACHE: OnceLock<Option<String>> = OnceLock::new();
    CACHE.get_or_init(computar).as_deref()
}

/// Marcadores em volta do valor: um `~/.zshrc` interativo pode imprimir banner,
/// fastfetch, aviso de update. Sem delimitador, esse lixo entraria no PATH.
#[cfg(not(windows))]
const INICIO: &str = "__SHVIA_PATH_INICIO__";
#[cfg(not(windows))]
const FIM: &str = "__SHVIA_PATH_FIM__";

/// Diretórios que quase sempre importam e que o PATH do launchd não tem. Entram
/// só se existirem, como rede de segurança para quando o shell não responder.
#[cfg(not(windows))]
fn extras() -> Vec<String> {
    let mut v = vec!["/opt/homebrew/bin".to_string(), "/usr/local/bin".to_string()];
    if let Ok(home) = std::env::var("HOME") {
        for sub in [".local/bin", ".cargo/bin", ".bun/bin", ".volta/bin"] {
            v.push(format!("{home}/{sub}"));
        }
    }
    v
}

#[cfg(not(windows))]
fn computar() -> Option<String> {
    let atual = std::env::var("PATH").unwrap_or_default();
    let base = do_shell_de_login().unwrap_or_else(|| atual.clone());

    // União: base + o PATH ATUAL + extras que existem e ainda não estão lá. Preserva a
    // ORDEM do usuário (quem põe rbenv/nvm antes do sistema faz isso de propósito).
    //
    // 🔴 O PATH atual entrou na união em 02/09/2026 (achado F-14 da revisão de 01/09). O
    // comentário aqui já dizia "união", e não era: quando o shell de login respondia, a
    // resposta dele **substituía** o PATH do processo em vez de somar. Quem abre o app pelo
    // terminal — com nvm, venv, ou o `bin` de um plugin na sessão — perdia esses diretórios
    // no sidecar, e o agente ficava insistindo num comando que "existe" no terminal do lado.
    //
    // O teste `computar_nunca_perde_o_que_ja_havia` afirmava exatamente este invariante e
    // estava **vermelho**; era o código que discordava do próprio comentário, não o teste.
    let mut saida: Vec<&str> = base.split(':').filter(|s| !s.is_empty()).collect();
    for dir in atual.split(':').filter(|s| !s.is_empty()) {
        if !saida.contains(&dir) {
            saida.push(dir);
        }
    }
    let extras = extras();
    for e in &extras {
        if !saida.contains(&e.as_str()) && std::path::Path::new(e).is_dir() {
            saida.push(e);
        }
    }
    let novo = saida.join(":");
    // Nada mudou → não mexe no env do filho (menos surpresa no diagnóstico).
    if novo == atual || novo.is_empty() {
        None
    } else {
        Some(novo)
    }
}

/// Pergunta o PATH ao shell de login **interativo** (`-ilc`): é aí que moram as
/// linhas de `~/.zshrc` (rbenv, nvm, composer). Com watchdog: um dotfile que
/// pendura o shell não pode pendurar o app.
#[cfg(not(windows))]
fn do_shell_de_login() -> Option<String> {
    let shell = std::env::var("SHELL").ok()?;
    if !shell.starts_with('/') || !std::path::Path::new(&shell).exists() {
        return None;
    }
    let script = format!("printf '{INICIO}%s{FIM}' \"$PATH\"");

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let out = std::process::Command::new(&shell).arg("-ilc").arg(&script).output();
        let _ = tx.send(out);
    });
    // 3s cobre um shell de login pesado com folga; estourou, a thread fica com o
    // filho pendurado (é uma vez por processo) e caímos nos extras.
    let out = rx.recv_timeout(std::time::Duration::from_secs(3)).ok()?.ok()?;

    let texto = String::from_utf8_lossy(&out.stdout);
    let valor = texto.split_once(INICIO)?.1.split_once(FIM)?.0;
    validar(valor)
}

/// Aceita só o que parece PATH: tamanho são, sem controle/NUL, com pelo menos um
/// diretório absoluto. Um dotfile quebrado devolvendo lixo não vira env do filho.
#[cfg(not(windows))]
fn validar(v: &str) -> Option<String> {
    if v.is_empty() || v.len() > 16 * 1024 {
        return None;
    }
    if v.chars().any(|c| c.is_control()) {
        return None;
    }
    if !v.split(':').any(|d| d.starts_with('/')) {
        return None;
    }
    Some(v.to_string())
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn validar_recusa_lixo_e_aceita_path() {
        assert!(validar("").is_none(), "vazio");
        assert!(validar("banner do zshrc\n/usr/bin").is_none(), "controle/newline");
        assert!(validar("naoabsoluto:relativo").is_none(), "sem diretório absoluto");
        assert!(validar(&"/x".repeat(20_000)).is_none(), "tamanho absurdo");
        assert_eq!(validar("/opt/homebrew/bin:/usr/bin").as_deref(), Some("/opt/homebrew/bin:/usr/bin"));
    }

    #[test]
    fn computar_nunca_perde_o_que_ja_havia() {
        // Qualquer que seja o resultado (shell real ou fallback), o PATH que já
        // estava no processo tem que sobreviver — perder diretório do usuário
        // seria trocar um bug por outro.
        let atual = std::env::var("PATH").unwrap_or_default();
        if let Some(novo) = computar() {
            for dir in atual.split(':').filter(|s| !s.is_empty()) {
                assert!(novo.split(':').any(|d| d == dir), "perdeu {dir}");
            }
        }
    }
}
