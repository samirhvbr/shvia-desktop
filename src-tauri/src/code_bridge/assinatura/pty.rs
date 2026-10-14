//! Running `claude setup-token` under a pseudo-terminal, and ending it (1.28.0).
//!
//! The client is an Ink screen: it draws on a terminal, wants raw keyboard input for the code a browser
//! may show, and prints the token once, as part of a frame. Pipes would not do (that is how `claude auth
//! login` was driven, and it has a plain prompt; this one does not). So the client gets a terminal of its
//! own, the way a terminal emulator would give it one, and this module reads the screen.
//!
//! One flow runs in the child's own session (`setsid`), so ending it is a signal to a process GROUP by id
//! and never a search by name: whatever the client started goes with it, and nothing else does.

use super::tela::{achar_token, achar_url, limpar_ansi, Segredo};
use super::{falha, Desfecho, Evento, Falha};
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The terminal is wide on purpose: the client wraps a long line at the terminal's width, and a token cut by
/// a wrap is the one thing this module must never hand on. 500 columns hold the URL and the token on one line.
const LINHAS: u16 = 50;
const COLUNAS: u16 = 500;

/// How much of the screen is kept. The client redraws a spinner while it waits, so the stream grows without
/// bound for as long as the person takes; only the end of it matters, and the end is what is kept.
const TETO_DA_TELA: usize = 256 * 1024;

const INTERVALO: Duration = Duration::from_millis(100);

/// How long the client gets to leave on its own after the token is read, and after a TERM signal.
const GRACA: Duration = Duration::from_secs(2);

/// What the client needs from the environment, and nothing else: a place to live, a locale, the desktop it
/// opens the browser on, the person's proxy and certificates. A key the app was launched with
/// (`ANTHROPIC_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`, a cloud credential, a ShvIA token) is not here, so the
/// client behaves the same whatever the shell that started the app had exported.
const AMBIENTE_PERMITIDO: &[&str] = &[
    "HOME", "USER", "LOGNAME", "SHELL", "LANG", "LANGUAGE", "TMPDIR", "DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY",
    "DBUS_SESSION_BUS_ADDRESS", "DESKTOP_SESSION", "BROWSER", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY",
    "http_proxy", "https_proxy", "all_proxy", "no_proxy", "SSL_CERT_FILE", "SSL_CERT_DIR", "NODE_EXTRA_CA_CERTS",
];
const PREFIXOS_PERMITIDOS: &[&str] = &["LC_", "XDG_"];

/// The environment the client is started with, from the environment the app has. `path` is the person's
/// real PATH (`user_env::sidecar_path`), which a GUI app launched from the Dock does not inherit.
pub(super) fn ambiente_do_filho(origem: &[(String, String)], path: Option<&str>) -> Vec<(String, String)> {
    let mut saida: Vec<(String, String)> = origem
        .iter()
        .filter(|(k, _)| AMBIENTE_PERMITIDO.contains(&k.as_str()) || PREFIXOS_PERMITIDOS.iter().any(|p| k.starts_with(p)))
        .cloned()
        .collect();
    let path = path
        .map(str::to_string)
        .or_else(|| origem.iter().find(|(k, _)| k == "PATH").map(|(_, v)| v.clone()));
    if let Some(p) = path {
        saida.push(("PATH".to_string(), p));
    }
    saida.push(("TERM".to_string(), "xterm-256color".to_string()));
    saida
}

/// The command, with that environment and the account the person picked. `programa` and `args` are
/// parameters so a test can start a stand-in; the bridge passes the `claude` it found and `setup-token`.
pub(super) fn montar_comando(
    programa: &std::path::Path,
    args: &[&str],
    conta: Option<&crate::contas_claude::Alvo>,
    origem: &[(String, String)],
    path: Option<&str>,
) -> Command {
    let mut cmd = crate::processo::comando(programa);
    cmd.args(args);
    cmd.env_clear();
    for (k, v) in ambiente_do_filho(origem, path) {
        cmd.env(k, v);
    }
    crate::contas_claude::aplicar(&mut cmd, conta);
    cmd
}

/// What the client has drawn so far, as raw bytes, and whether it has finished. The end of the stream is kept
/// when it grows past the ceiling, and the bytes are overwritten when this is dropped: the token is in them.
#[derive(Default)]
struct Dados {
    bytes: Vec<u8>,
    fim: bool,
}

impl Drop for Dados {
    fn drop(&mut self) {
        for b in self.bytes.iter_mut() {
            // SAFETY: a valid, aligned, exclusive reference; volatile so the write is not optimised away.
            unsafe { std::ptr::write_volatile(b, 0) };
        }
    }
}

#[derive(Default)]
struct Tela {
    dados: Mutex<Dados>,
}

impl Tela {
    fn anexar(&self, novos: &[u8]) {
        if let Ok(mut d) = self.dados.lock() {
            d.bytes.extend_from_slice(novos);
            // Trimmed in steps, not byte by byte: a drain from the front of a full buffer on every read is
            // quadratic while a spinner is drawing.
            if d.bytes.len() > TETO_DA_TELA + TETO_DA_TELA / 2 {
                let excesso = d.bytes.len() - TETO_DA_TELA;
                d.bytes.drain(..excesso);
            }
        }
    }

    fn terminar(&self) {
        if let Ok(mut d) = self.dados.lock() {
            d.fim = true;
        }
    }

    fn instantaneo(&self) -> (Vec<u8>, bool) {
        self.dados.lock().map(|d| (d.bytes.clone(), d.fim)).unwrap_or_default()
    }
}

/// The handle other threads hold on a running flow: ask it to stop, or type a line into its terminal.
pub(super) struct Controle {
    cancelado: AtomicBool,
    /// 0 = no process (not started yet, or already reaped and not to be signalled).
    pid: AtomicU32,
    escrita: Mutex<Option<File>>,
}

impl Controle {
    fn novo() -> Self {
        Self { cancelado: AtomicBool::new(false), pid: AtomicU32::new(0), escrita: Mutex::new(None) }
    }

    /// Stops the flow: the worker sees the flag within a tick, and the signal goes out at once so the app can
    /// quit without waiting for that tick (`RunEvent::Exit` calls this).
    pub(super) fn cancelar(&self) {
        self.cancelado.store(true, Ordering::SeqCst);
        let pid = self.pid.load(Ordering::SeqCst);
        if pid != 0 {
            sinal_ao_grupo(pid, libc::SIGTERM);
        }
    }

    fn esta_cancelado(&self) -> bool {
        self.cancelado.load(Ordering::SeqCst)
    }

    /// Types `texto` and Enter into the client's terminal: the code the browser showed, when it could not hand
    /// it back by itself. Written, never kept, never echoed to the page.
    pub(super) fn digitar(&self, texto: &str) -> Result<(), Falha> {
        let mut guarda = self.escrita.lock().map_err(|_| falha("estado_perdido", "Estado da conexão inacessível."))?;
        let Some(mestre) = guarda.as_mut() else {
            return Err(falha("sem_fluxo", "Não há conexão em andamento. Comece de novo."));
        };
        mestre
            .write_all(texto.as_bytes())
            .and_then(|()| mestre.write_all(b"\r"))
            .map_err(|_| falha("escrita_falhou", "Não consegui entregar o código ao Claude Code."))
    }
}

/// A signal to the child's process group. The child called `setsid`, so its group id is its pid.
fn sinal_ao_grupo(pid: u32, sinal: libc::c_int) {
    // SAFETY: kill(2) takes plain integers. A negative pid addresses the group, and the group is the child's
    // own: it is only ever called for a pid this process started and has not yet waited on.
    unsafe {
        libc::kill(-(pid as libc::pid_t), sinal);
    }
}

fn abrir_pty() -> std::io::Result<(OwnedFd, OwnedFd)> {
    let mut mestre: libc::c_int = -1;
    let mut escravo: libc::c_int = -1;
    let tamanho = libc::winsize { ws_row: LINHAS, ws_col: COLUNAS, ws_xpixel: 0, ws_ypixel: 0 };
    // SAFETY: both out-pointers are valid for the call, and openpty accepts a null name and a null termios.
    let r = unsafe { libc::openpty(&mut mestre, &mut escravo, std::ptr::null_mut(), std::ptr::null(), &tamanho) };
    if r != 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: openpty returned two fresh descriptors that nothing else owns.
    let (mestre, escravo) = unsafe { (OwnedFd::from_raw_fd(mestre), OwnedFd::from_raw_fd(escravo)) };
    fechar_na_exec(&mestre)?;
    fechar_na_exec(&escravo)?;
    Ok((mestre, escravo))
}

fn fechar_na_exec(fd: &OwnedFd) -> std::io::Result<()> {
    // SAFETY: a plain fcntl on a descriptor this function's caller owns.
    let r = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) };
    if r < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// A started client: the child, what it has drawn, and the handle for the other threads.
pub(super) struct Sessao {
    filho: Child,
    tela: Arc<Tela>,
    pub(super) controle: Arc<Controle>,
}

impl Sessao {
    /// Ends the client and everything it started: out on its own if it is on its way, else TERM to the group,
    /// then KILL. Safe to call again, and called on drop.
    fn encerrar(&mut self) {
        if matches!(self.filho.try_wait(), Ok(None)) {
            let pid = self.filho.id();
            if !self.esperar_saida(GRACA) {
                sinal_ao_grupo(pid, libc::SIGTERM);
                if !self.esperar_saida(GRACA) {
                    sinal_ao_grupo(pid, libc::SIGKILL);
                    let _ = self.filho.wait();
                }
            }
        }
        self.controle.pid.store(0, Ordering::SeqCst);
        // Closing the master is also a hang-up for whatever is left on that terminal.
        if let Ok(mut e) = self.controle.escrita.lock() {
            *e = None;
        }
    }

    fn esperar_saida(&mut self, prazo: Duration) -> bool {
        let inicio = Instant::now();
        loop {
            if matches!(self.filho.try_wait(), Ok(Some(_))) {
                return true;
            }
            if inicio.elapsed() >= prazo {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Ends it at once: the caller wanted it gone (cancelled, out of time), so there is no grace to give.
    fn matar(&mut self) {
        if matches!(self.filho.try_wait(), Ok(None)) {
            let pid = self.filho.id();
            sinal_ao_grupo(pid, libc::SIGTERM);
            if !self.esperar_saida(Duration::from_millis(1500)) {
                sinal_ao_grupo(pid, libc::SIGKILL);
                let _ = self.filho.wait();
            }
        }
        self.controle.pid.store(0, Ordering::SeqCst);
        if let Ok(mut e) = self.controle.escrita.lock() {
            *e = None;
        }
    }
}

impl Drop for Sessao {
    fn drop(&mut self) {
        self.matar();
    }
}

/// Starts `cmd` on a terminal of its own, with that terminal as its controlling one.
pub(super) fn abrir_sessao(mut cmd: Command) -> Result<Sessao, Falha> {
    let sem_terminal = || falha("pty_falhou", "Não consegui abrir um terminal para o Claude Code.");
    let (mestre, escravo) = abrir_pty().map_err(|_| sem_terminal())?;
    // stdin, stdout and stderr are the same terminal: three descriptors of the slave.
    let dup = |fd: &OwnedFd| fd.try_clone().map(Stdio::from).map_err(|_| sem_terminal());
    cmd.stdin(dup(&escravo)?).stdout(dup(&escravo)?).stderr(dup(&escravo)?);
    // SAFETY: the closure runs between fork and exec and calls only async-signal-safe functions (setsid,
    // ioctl); it allocates nothing. A failure aborts the spawn with the OS error.
    unsafe {
        cmd.pre_exec(|| {
            // A session of its own, so the slave can become its controlling terminal.
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            // Descriptor 0 is the slave: the standard handoff of a controlling terminal.
            if libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let filho = cmd
        .spawn()
        .map_err(|_| falha("spawn_falhou", "Não consegui iniciar o Claude Code (`claude setup-token`)."))?;
    // `cmd` holds the slave descriptors it was given; while any is open the master never sees the end of the
    // stream, so both go before anything reads.
    drop(cmd);
    drop(escravo);

    let leitura = File::from(mestre.try_clone().map_err(|_| sem_terminal())?);
    let tela = Arc::new(Tela::default());
    let controle = Arc::new(Controle::novo());
    controle.pid.store(filho.id(), Ordering::SeqCst);
    if let Ok(mut e) = controle.escrita.lock() {
        *e = Some(File::from(mestre));
    }
    let da_tela = tela.clone();
    std::thread::spawn(move || {
        let mut leitura = leitura;
        let mut buf = [0u8; 4096];
        loop {
            match leitura.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => da_tela.anexar(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                // EIO is how Linux says the other side of the terminal closed.
                Err(_) => break,
            }
        }
        da_tela.terminar();
    });
    Ok(Sessao { filho, tela, controle })
}

/// Drives one flow to its end on the calling thread: waits for the person to authorise, reads the token off the
/// screen, hands it to `entregar`, and tells `emitir` each step. Returns how it ended.
///
/// The token is passed by reference to `entregar` and dropped here; `emitir` is never given anything read from the
/// screen except the sign-in URL (a host check lets only Claude's own through).
pub(super) fn conduzir<E, F>(mut sessao: Sessao, prazo: Duration, entregar: F, emitir: E) -> Desfecho
where
    E: Fn(Evento),
    F: FnOnce(&Segredo) -> Result<(), Falha>,
{
    let inicio = Instant::now();
    let mut url_dada = false;
    let mut saiu_em: Option<Instant> = None;
    emitir(Evento::Aguardando { url: None });
    loop {
        if sessao.controle.esta_cancelado() {
            sessao.matar();
            emitir(Evento::Cancelado);
            return Desfecho::Cancelado;
        }
        if inicio.elapsed() >= prazo {
            sessao.matar();
            emitir(Evento::Falhou(falha(
                "tempo_esgotado",
                "A autorização não foi concluída a tempo. Comece de novo quando puder autorizar.",
            )));
            return Desfecho::Falhou("tempo_esgotado");
        }
        let (bytes, fim) = sessao.tela.instantaneo();
        let texto = limpar_ansi(&bytes);
        if !url_dada {
            if let Some(url) = achar_url(&texto) {
                url_dada = true;
                emitir(Evento::Aguardando { url: Some(url) });
            }
        }
        if saiu_em.is_none() && matches!(sessao.filho.try_wait(), Ok(Some(_))) {
            saiu_em = Some(Instant::now());
            sessao.controle.pid.store(0, Ordering::SeqCst);
        }
        // Everything the client wrote has been read: it closed the terminal, or it left a second ago (a
        // process that stayed on the terminal would keep the read open forever).
        let terminou = fim || saiu_em.is_some_and(|t| t.elapsed() > Duration::from_secs(1));
        if let Some(token) = achar_token(&texto, terminou) {
            emitir(Evento::Enviando);
            let resultado = entregar(&token);
            sessao.encerrar();
            return match resultado {
                Ok(()) => {
                    emitir(Evento::Conectado);
                    Desfecho::Conectado
                }
                Err(f) => {
                    let codigo = f.codigo;
                    emitir(Evento::Falhou(f));
                    Desfecho::Falhou(codigo)
                }
            };
        }
        if terminou && saiu_em.is_some() {
            emitir(Evento::Falhou(falha(
                "sem_token",
                "O Claude Code terminou sem entregar um token que eu reconheça. Rode `claude setup-token` no terminal e cole o token em Configurações → Motores.",
            )));
            return Desfecho::Falhou("sem_token");
        }
        std::thread::sleep(INTERVALO);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;

    const TOKEN: &str = "sk-ant-oat01-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCd";

    /// A stand-in for `claude`: `sh -c <script>` through the same builder the bridge uses, so it gets the same
    /// terminal and the same lean environment. `PATH` is what the script needs (printf is a builtin; sleep, stty,
    /// tty and env are not).
    fn ficticio(script: &str) -> Command {
        let origem = [("PATH".to_string(), "/usr/bin:/bin".to_string())];
        montar_comando(Path::new("sh"), &["-c", script], None, &origem, None)
    }

    /// What the real client draws on success (shape from its bundled source, 2.1.300), with the codes of a terminal.
    fn quadro_de_sucesso(token_impresso: &str) -> String {
        format!(
            "printf '\\033[2K\\033[1A\\033[32m✓ Long-lived authentication token created successfully!\\033[39m\\r\\n\\r\\n\
             Your OAuth token (valid for 1 year):\\r\\n\\r\\n\\033[33m{token_impresso}\\033[39m\\r\\n\\r\\n\
             Store this token securely. You won'\"'\"'t be able to see it again.\\r\\n'"
        )
    }

    /// The first screen: the sign-in URL as an OSC 8 hyperlink, as the client shows it.
    const PRIMEIRA_TELA: &str = "printf '\\033[?25l\\033[1mThis will guide you through long-lived (1-year) auth token setup\\033[22m\\r\\n\
        Browser didn'\"'\"'t open? Use the url below to sign in \\033]8;;https://claude.com/cai/oauth/authorize?code=true&state=QZiX\\033\\\\https://claude.com/cai/oauth/authorize?code=true&state=QZiX\\033]8;;\\033\\\\\\r\\n'";

    fn pasta_de_teste(nome: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("shvia-assinatura-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// Reads the whole screen of a short-lived stand-in: waits for the end of the stream.
    fn tela_inteira(sessao: &Sessao) -> String {
        let inicio = Instant::now();
        loop {
            let (bytes, fim) = sessao.tela.instantaneo();
            if fim || inicio.elapsed() > Duration::from_secs(15) {
                assert!(fim, "o filho não terminou: {}", limpar_ansi(&bytes));
                return limpar_ansi(&bytes);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn vivo(pid: i32) -> bool {
        // SAFETY: signal 0 only checks that the process exists.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    fn espera_morrer(pid: i32) -> bool {
        let inicio = Instant::now();
        while inicio.elapsed() < Duration::from_secs(5) {
            if !vivo(pid) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    fn pid_de(pasta: &Path, arquivo: &str) -> i32 {
        for _ in 0..100 {
            if let Ok(t) = std::fs::read_to_string(pasta.join(arquivo)) {
                if let Ok(n) = t.trim().parse() {
                    return n;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("o filho não escreveu {arquivo}");
    }

    #[test]
    fn o_filho_tem_um_terminal_de_verdade_e_largo() {
        let sessao = abrir_sessao(ficticio(
            "[ -t 0 ] && [ -t 1 ] && [ -t 2 ] || { echo SEMTTY; exit 3; }; \
             echo tamanho=$(stty size); echo tty=$(tty); echo TERM=$TERM; \
             if (: </dev/tty) 2>/dev/null; then echo ctty=sim; else echo ctty=nao; fi",
        ))
        .expect("abre");
        let tela = tela_inteira(&sessao);
        assert!(!tela.contains("SEMTTY"), "{tela}");
        assert!(tela.contains("tamanho=50 500"), "a largura é a que impede a quebra do token: {tela}");
        assert!(tela.contains("tty=/dev/"), "{tela}");
        assert!(tela.contains("TERM=xterm-256color"), "{tela}");
        assert!(tela.contains("ctty=sim"), "o terminal é o de controle: sem isso o Ink não liga o modo cru: {tela}");
    }

    #[test]
    fn o_ambiente_do_filho_e_enxuto_e_leva_a_conta() {
        use crate::contas_claude::{Alvo, Var};
        let origem: Vec<(String, String)> = [
            ("HOME", "/home/pessoa"), ("LANG", "pt_BR.UTF-8"), ("LC_ALL", "pt_BR.UTF-8"), ("XDG_RUNTIME_DIR", "/run/user/1000"),
            ("DISPLAY", ":0"), ("HTTPS_PROXY", "http://proxy:3128"), ("PATH", "/usr/bin:/bin"),
            ("ANTHROPIC_API_KEY", "segredo-1"), ("CLAUDE_CODE_OAUTH_TOKEN", "segredo-2"), ("AWS_SECRET_ACCESS_KEY", "segredo-3"),
            ("GITHUB_TOKEN", "segredo-4"), ("SHVIA_TOKEN", "segredo-5"), ("CLAUDE_CONFIG_DIR", "/outra/conta"), ("TERM", "dumb"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let alvo = Alvo { var: Var::SecureStorage, dir: "/tmp/conta-teste".into() };
        let cmd = montar_comando(Path::new("sh"), &["-c", "env | sort"], Some(&alvo), &origem, Some("/opt/bin:/usr/bin:/bin"));
        let sessao = abrir_sessao(cmd).expect("abre");
        let tela = tela_inteira(&sessao);
        for presente in ["HOME=/home/pessoa", "LANG=pt_BR.UTF-8", "LC_ALL=pt_BR.UTF-8", "XDG_RUNTIME_DIR=/run/user/1000", "DISPLAY=:0",
            "HTTPS_PROXY=http://proxy:3128", "PATH=/opt/bin:/usr/bin:/bin", "TERM=xterm-256color", "CLAUDE_SECURESTORAGE_CONFIG_DIR=/tmp/conta-teste"] {
            assert!(tela.contains(presente), "falta {presente}: {tela}");
        }
        for ausente in ["segredo-", "ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN", "AWS_", "GITHUB_TOKEN", "SHVIA_TOKEN", "/outra/conta", "TERM=dumb"] {
            assert!(!tela.contains(ausente), "{ausente} vazou para o filho: {tela}");
        }
        // The CLOSED set: every variable the child sees is one of these (plus what a shell adds to itself). A child
        // that inherited the app's own environment would show dozens more, and this is what turns that red.
        let esperadas = [
            "HOME", "LANG", "LC_ALL", "XDG_RUNTIME_DIR", "DISPLAY", "HTTPS_PROXY", "PATH", "TERM", "CLAUDE_SECURESTORAGE_CONFIG_DIR",
            "PWD", "OLDPWD", "SHLVL", "_",
        ];
        for linha in tela.lines() {
            if let Some((nome, _)) = linha.split_once('=') {
                if nome.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') && !nome.is_empty() {
                    assert!(esperadas.contains(&nome), "variável fora da lista no filho: {nome}\n{tela}");
                }
            }
        }
    }

    /// SOURCE check, declared as such: the environment is cleared before it is filled. A test that exercises the
    /// builder proves what the allowlist lets through; only the source says the app's own environment is not the
    /// base the allowlist is subtracted from.
    #[test]
    fn o_comando_limpa_o_ambiente_antes_de_enche_lo() {
        let fonte = include_str!("pty.rs");
        let corpo = fonte.split("pub(super) fn montar_comando").nth(1).and_then(|x| x.split("\n}\n").next()).unwrap_or("");
        let limpa = corpo.find(&["env_", "clear()"].concat()).expect("env_clear sumiu");
        let enche = corpo.find(".env(k, v)").expect("o laço que enche sumiu");
        assert!(limpa < enche, "o ambiente do app seria a base");
    }

    fn rodar<F>(script: &str, prazo: Duration, entregar: F) -> (Desfecho, Vec<Evento>)
    where
        F: FnOnce(&Segredo) -> Result<(), Falha>,
    {
        let sessao = abrir_sessao(ficticio(script)).expect("abre");
        let eventos = Mutex::new(Vec::new());
        let desfecho = conduzir(sessao, prazo, entregar, |e| eventos.lock().unwrap().push(e));
        (desfecho, eventos.into_inner().unwrap())
    }

    #[test]
    fn o_fluxo_inteiro_le_o_token_entrega_e_conta_cada_passo() {
        let pasta = pasta_de_teste("fluxo");
        let script = format!(
            "echo $$ > {p}/filho; {primeira}; sleep 0.4; {sucesso}; sleep 0.2",
            p = pasta.display(),
            primeira = PRIMEIRA_TELA,
            sucesso = quadro_de_sucesso(TOKEN)
        );
        let entregue = Mutex::new(None::<String>);
        let (desfecho, eventos) = rodar(&script, Duration::from_secs(30), |t| {
            *entregue.lock().unwrap() = Some(t.como_str().to_string());
            Ok(())
        });
        assert_eq!(desfecho, Desfecho::Conectado, "{eventos:?}");
        assert_eq!(entregue.into_inner().unwrap().as_deref(), Some(TOKEN));
        assert_eq!(
            eventos,
            vec![
                Evento::Aguardando { url: None },
                Evento::Aguardando { url: Some("https://claude.com/cai/oauth/authorize?code=true&state=QZiX".to_string()) },
                Evento::Enviando,
                Evento::Conectado,
            ]
        );
        // 🔴 Nothing the page is told carries the token.
        for e in &eventos {
            let j = super::super::para_json(1, e).to_string();
            assert!(!j.contains("sk-ant") && !j.contains("AbCdEfGh"), "{j}");
        }
        assert!(espera_morrer(pid_de(&pasta, "filho")), "o cliente não ficou vivo depois de conectar");
        let _ = std::fs::remove_dir_all(&pasta);
    }

    /// A terminal that wraps the token: the line break inside it is put back, and the whole token is what is sent.
    #[test]
    fn um_token_quebrado_em_duas_linhas_e_entregue_inteiro() {
        let (a, b) = TOKEN.split_at(60);
        let entregue = Mutex::new(None::<String>);
        let (desfecho, _) = rodar(&quadro_de_sucesso(&format!("{a}\\r\\n{b}")), Duration::from_secs(30), |t| {
            *entregue.lock().unwrap() = Some(t.como_str().to_string());
            Ok(())
        });
        assert_eq!(desfecho, Desfecho::Conectado);
        assert_eq!(entregue.into_inner().unwrap().as_deref(), Some(TOKEN));
    }

    #[test]
    fn terminar_sem_token_e_falha_e_nada_e_entregue() {
        let chamou = AtomicBool::new(false);
        let (desfecho, eventos) = rodar(
            "printf 'Something went wrong\\r\\n'; exit 0",
            Duration::from_secs(30),
            |_| {
                chamou.store(true, Ordering::SeqCst);
                Ok(())
            },
        );
        assert_eq!(desfecho, Desfecho::Falhou("sem_token"));
        assert!(!chamou.load(Ordering::SeqCst), "sem token não há o que entregar");
        assert!(matches!(eventos.last(), Some(Evento::Falhou(f)) if f.codigo == "sem_token"), "{eventos:?}");
    }

    /// The API key an old client could print is not a subscription token: not read, not sent.
    #[test]
    fn uma_chave_de_api_no_lugar_do_token_nao_e_entregue() {
        let chave = "sk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789";
        let chamou = AtomicBool::new(false);
        let (desfecho, _) = rodar(&format!("{}; exit 0", quadro_de_sucesso(chave)), Duration::from_secs(30), |_| {
            chamou.store(true, Ordering::SeqCst);
            Ok(())
        });
        assert_eq!(desfecho, Desfecho::Falhou("sem_token"));
        assert!(!chamou.load(Ordering::SeqCst));
    }

    #[test]
    fn a_falha_da_entrega_chega_como_falha_e_o_cliente_e_encerrado() {
        let pasta = pasta_de_teste("entrega");
        let script = format!("echo $$ > {p}/filho; {s}; sleep 30", p = pasta.display(), s = quadro_de_sucesso(TOKEN));
        let (desfecho, eventos) = rodar(&script, Duration::from_secs(60), |_| {
            Err(falha("servidor_recusou", "O ShvIA não aceitou o token."))
        });
        assert_eq!(desfecho, Desfecho::Falhou("servidor_recusou"));
        assert_eq!(eventos.last(), Some(&Evento::Falhou(falha("servidor_recusou", "O ShvIA não aceitou o token."))));
        assert!(espera_morrer(pid_de(&pasta, "filho")), "o cliente que ficou esperando foi encerrado");
        let _ = std::fs::remove_dir_all(&pasta);
    }

    #[test]
    fn o_tempo_esgotado_mata_o_filho_e_o_que_ele_iniciou() {
        let pasta = pasta_de_teste("prazo");
        // The grandchild ignores the hang-up, like anything started with `nohup`: closing the terminal does not reach
        // it, so only the signal to the group does. Without that, the terminal's own hang-up would end it and the test
        // could not tell a group kill from a kill of the child alone.
        let script = format!("(trap '' HUP; exec sleep 60) & echo $! > {p}/neto; echo $$ > {p}/filho; wait", p = pasta.display());
        let inicio = Instant::now();
        let (desfecho, eventos) = rodar(&script, Duration::from_secs(1), |_| Ok(()));
        assert_eq!(desfecho, Desfecho::Falhou("tempo_esgotado"));
        assert!(matches!(eventos.last(), Some(Evento::Falhou(f)) if f.codigo == "tempo_esgotado"));
        assert!(inicio.elapsed() < Duration::from_secs(10), "{:?}", inicio.elapsed());
        assert!(espera_morrer(pid_de(&pasta, "filho")), "o filho ficou vivo");
        assert!(espera_morrer(pid_de(&pasta, "neto")), "o que o filho iniciou ficou vivo: o grupo é o que se encerra");
        let _ = std::fs::remove_dir_all(&pasta);
    }

    #[test]
    fn cancelar_encerra_e_avisa() {
        let pasta = pasta_de_teste("cancelar");
        let script = format!("(trap '' HUP; exec sleep 60) & echo $! > {p}/neto; echo $$ > {p}/filho; wait", p = pasta.display());
        let sessao = abrir_sessao(ficticio(&script)).expect("abre");
        let controle = sessao.controle.clone();
        let eventos = Arc::new(Mutex::new(Vec::new()));
        let dos_eventos = eventos.clone();
        let fio = std::thread::spawn(move || {
            conduzir(sessao, Duration::from_secs(60), |_| Ok(()), move |e| dos_eventos.lock().unwrap().push(e))
        });
        let (filho, neto) = (pid_de(&pasta, "filho"), pid_de(&pasta, "neto"));
        controle.cancelar();
        assert_eq!(fio.join().unwrap(), Desfecho::Cancelado);
        assert_eq!(eventos.lock().unwrap().last(), Some(&Evento::Cancelado));
        assert!(espera_morrer(filho) && espera_morrer(neto), "ficou processo vivo depois de cancelar");
        let _ = std::fs::remove_dir_all(&pasta);
    }

    /// A client that ignores the polite signal is not waited on forever.
    #[test]
    fn quem_ignora_o_term_leva_o_kill() {
        let pasta = pasta_de_teste("kill");
        let script = format!("trap '' TERM; echo $$ > {p}/filho; sleep 60 & echo $! > {p}/neto; wait", p = pasta.display());
        let inicio = Instant::now();
        let (desfecho, _) = rodar(&script, Duration::from_secs(1), |_| Ok(()));
        assert_eq!(desfecho, Desfecho::Falhou("tempo_esgotado"));
        assert!(inicio.elapsed() < Duration::from_secs(15), "{:?}", inicio.elapsed());
        assert!(espera_morrer(pid_de(&pasta, "filho")) && espera_morrer(pid_de(&pasta, "neto")));
        let _ = std::fs::remove_dir_all(&pasta);
    }

    /// The code the browser shows is typed into the client's terminal, and only there.
    #[test]
    fn o_codigo_colado_chega_ao_terminal_do_cliente() {
        let script = format!(
            "printf 'Paste code here if prompted > '; IFS= read -r codigo; \
             if [ \"$codigo\" = CODIGO-DE-TESTE ]; then {s}; else echo CODIGO-ERRADO; fi",
            s = quadro_de_sucesso(TOKEN)
        );
        let sessao = abrir_sessao(ficticio(&script)).expect("abre");
        let controle = sessao.controle.clone();
        let eventos = Arc::new(Mutex::new(Vec::new()));
        let dos_eventos = eventos.clone();
        let entregue = Arc::new(Mutex::new(None::<String>));
        let da_entrega = entregue.clone();
        let fio = std::thread::spawn(move || {
            conduzir(
                sessao,
                Duration::from_secs(60),
                move |t| {
                    *da_entrega.lock().unwrap() = Some(t.como_str().to_string());
                    Ok(())
                },
                move |e| dos_eventos.lock().unwrap().push(e),
            )
        });
        std::thread::sleep(Duration::from_millis(400));
        controle.digitar("CODIGO-DE-TESTE").expect("digita");
        assert_eq!(fio.join().unwrap(), Desfecho::Conectado);
        assert_eq!(entregue.lock().unwrap().as_deref(), Some(TOKEN));
        let json: String = eventos.lock().unwrap().iter().map(|e| super::super::para_json(1, e).to_string()).collect();
        assert!(!json.contains("CODIGO-DE-TESTE"), "o código não volta para a página: {json}");
    }

    #[test]
    fn digitar_sem_fluxo_e_recusado() {
        let controle = Controle::novo();
        assert_eq!(controle.digitar("x").unwrap_err().codigo, "sem_fluxo");
    }

    #[test]
    fn um_binario_que_nao_existe_e_falha_de_inicio() {
        let origem = [("PATH".to_string(), "/usr/bin:/bin".to_string())];
        let cmd = montar_comando(Path::new("/nao/existe/claude"), &["setup-token"], None, &origem, None);
        assert_eq!(abrir_sessao(cmd).err().map(|f| f.codigo), Some("spawn_falhou"));
    }

    /// The screen keeps its end, not its beginning: the token is drawn last.
    #[test]
    fn a_tela_guarda_o_fim() {
        let tela = Tela::default();
        let bloco = vec![b'x'; 100 * 1024];
        for _ in 0..6 {
            tela.anexar(&bloco);
        }
        tela.anexar(b"FIM");
        let (bytes, _) = tela.instantaneo();
        assert!(bytes.len() <= TETO_DA_TELA + TETO_DA_TELA / 2 + 4, "{}", bytes.len());
        assert!(bytes.ends_with(b"FIM"));
    }
}
