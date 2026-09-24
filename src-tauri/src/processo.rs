//! Starting a process: the one way this app builds a `Command` (1.6.57).
//!
//! The release build is `windows_subsystem = "windows"` (main.rs), a GUI app with no console of
//! its own. On Windows, every console program such an app starts gets a console WINDOW of its
//! own unless it is created with `CREATE_NO_WINDOW`: a black window for each `git status` of the
//! Changes tab, and one for the whole life of an engine session (`anna`, or `node` running a
//! runner). None of the app's process starts set the flag until 1.6.57. Nobody had seen it,
//! because the app had never run on a Windows machine; this was found by reading, while making
//! the runners work there.
//!
//! So every `Command` comes from `comando()`, tests included — the flag only exists on Windows,
//! and one rule with no exceptions is what the ruler below can hold: `Command::new(` appears in
//! this file and nowhere else.

use std::ffi::OsStr;
use std::process::Command;

/// `CREATE_NO_WINDOW` from the Win32 process creation flags.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A `Command` for `programa` that, on Windows, never opens a console window.
pub(crate) fn comando<S: AsRef<OsStr>>(programa: S) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(programa);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

#[cfg(test)]
mod tests {
    /// 🔴 `Command::new(` lives here and nowhere else (1.6.57). A process started any other way
    /// opens a console window on Windows, and CI would not notice: it never runs the app there.
    ///
    /// Every `.rs` under `src/` is read, tests included, with whole-line comments dropped (a
    /// comment may name the call to explain it). This file is the one exception, because the
    /// ruler and the constructor both have to say the name.
    #[test]
    fn todo_processo_nasce_pelo_construtor_sem_janela() {
        fn arquivos(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir).expect("src/ legível").flatten() {
                let p = e.path();
                if p.is_dir() {
                    arquivos(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push(p);
                }
            }
        }
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut todos = Vec::new();
        arquivos(&src, &mut todos);
        assert!(todos.len() > 10, "a régua leu {} arquivo(s): estaria medindo o vazio", todos.len());

        let agulha = ["Command", "::new("].concat();
        let mut fora = Vec::new();
        for p in &todos {
            if p.file_name().is_some_and(|n| n == "processo.rs") {
                continue;
            }
            let texto = std::fs::read_to_string(p).expect("fonte legível");
            for (i, linha) in texto.lines().enumerate() {
                if !linha.trim_start().starts_with("//") && linha.contains(&agulha) {
                    fora.push(format!("{}:{}", p.strip_prefix(&src).unwrap_or(p).display(), i + 1));
                }
            }
        }
        assert!(
            fora.is_empty(),
            "processo criado sem `crate::processo::comando` — no Windows ele abre uma janela de console: {fora:?}",
        );
    }
}
