//! Device commands for the agent (`aparelho-no`, ADR-039): the desktop half.
//!
//! The agent runs on the server. When it wants something only this machine has — show a
//! notification, read a file the person chooses, see the screen — the page asks here through
//! the `aparelho` action, with the command and the agent's stated reason, and sends the answer
//! back to the turn. That round trip is SHVIA-WEB's half; this file is what the machine allows.
//!
//! Three gates, and the page holds none of them:
//!
//! 1. **The switch in the tray** ("Comandos de aparelho para o agente"), off by default. Only a
//!    click in the tray turns it on: no bridge action writes it, so a script on the page cannot.
//!    Off, every command answers `desligado` and nothing happens on the machine.
//! 2. **A closed list** (`COMANDOS`). An unknown name is refused before anything runs.
//! 3. **Consent per command, where the command takes something.** `device.info` and
//!    `system.notify` give nothing away and run without asking, as in OpenClaw's defaults.
//!    `files.pick` opens the native picker, and choosing IS the consent: cancel and nothing is
//!    read. `screen.snapshot` asks in a native dialog EVERY time, naming the agent's reason as
//!    the agent's (unverified) words — there is no "always allow".

use super::*;

/// Every command this build answers, in the order `aparelhoStatus` lists them.
pub(crate) const COMANDOS: [&str; 4] = ["device.info", "system.notify", "files.pick", "screen.snapshot"];

/// The ceiling of a screen capture, the same 10 MB as an attachment (`MAX_PICK_BYTES`): the
/// page uploads the image to the conversation, and the server refuses a larger file anyway.
const MAX_TELA_BYTES: usize = MAX_PICK_BYTES;

/// How long a capture may take. On Linux it includes the portal's own permission dialog, which
/// GNOME shows the first time an app asks, so a person has time to read it.
const PRAZO_TELA: std::time::Duration = std::time::Duration::from_secs(90);

/// How long a request may wait on the person before the desktop refuses to act on it (`expirou`).
///
/// The server stops waiting after `prazo_s` — 180 s for `files.pick` and `screen.snapshot`
/// (`config('agent.aparelho.prazo_s')` in SHVIA-WEB) — and the agent moves on. A native dialog cannot
/// be closed from outside, so it stays on screen: a person who walked away and comes back would
/// click "Capturar a tela" for a request nobody is waiting for, and the image would be taken (and
/// posted) for nothing. This ceiling sits BELOW the server's, with a margin for the trip, so the
/// click of a dead request does nothing. It must stay under that `prazo_s`; ADR-039 says so.
pub(crate) const PRAZO_DO_PEDIDO: std::time::Duration = std::time::Duration::from_secs(170);

/// Is a request this old past its ceiling? Apart so the boundary can be tested.
pub(crate) fn vencido(decorrido: std::time::Duration, teto: std::time::Duration) -> bool {
    decorrido > teto
}

/// The ceiling of THIS request: the smaller of `PRAZO_DO_PEDIDO` and the seconds the page says are
/// left (`restante`, the fourth argument of `aparelho(...)`, SHVIA-WEB #454). `None` means it has
/// already run out (`restante` of 0 or less), so nothing may open at all.
///
/// The page computes `restante` as the server's `prazo_s` minus what the page already spent minus
/// 10 s for the stream's delivery, which it cannot measure; the fixed 170 s alone left a margin of
/// 10 s minus the latency (found when the two halves were cross-read). A shell older than this one
/// ignores the argument, which is why the page can send it already. Absent, or anything that is
/// not an integer, means "use my own 170": the stricter-or-equal side, never a longer wait.
pub(crate) fn teto_do_pedido(restante: Option<&serde_json::Value>) -> Option<std::time::Duration> {
    match restante.and_then(|r| r.as_i64()) {
        Some(s) if s <= 0 => None,
        Some(s) => Some(std::time::Duration::from_secs(s as u64).min(PRAZO_DO_PEDIDO)),
        None => Some(PRAZO_DO_PEDIDO),
    }
}

/// The ceiling in words for the capture dialog: minutes when it is long, seconds when it is short.
pub(super) fn prazo_por_extenso(teto: std::time::Duration) -> String {
    let s = teto.as_secs();
    if s >= 90 {
        format!("cerca de {} minutos", (s + 30) / 60)
    } else {
        format!("{s} segundos")
    }
}

/// `aparelho` — the one action the agent's device commands come through.
pub(super) fn aparelho(window: &WebviewWindow, req: &str, v: &serde_json::Value) {
    let comando = v.get("comando").and_then(|x| x.as_str()).unwrap_or_default();
    let motivo = motivo_limpo(v.get("motivo").and_then(|x| x.as_str()).unwrap_or_default());
    // The clock of the request starts when it reaches the desktop, before any dialog.
    let inicio = std::time::Instant::now();
    if !crate::tray::aparelho_ligado(window.app_handle()) {
        return reply(window, req, false, falha("desligado",
            "Os comandos de aparelho estão desligados. Ligue em “Comandos de aparelho para o agente”, no ícone do ShvIA na bandeja."));
    }
    // The page says how long the request still has; never longer than our own ceiling. Already out:
    // no dialog, no picker, nothing opens for a request the tool has given up on.
    let Some(teto) = teto_do_pedido(v.get("restante")) else {
        return reply(window, req, false, falha("expirou", "o pedido já expirou — nada foi feito"));
    };
    match comando {
        "device.info" => reply(window, req, true, info(crate::tray::navegador_ligado(window.app_handle()))),
        "system.notify" => {
            let args = v.get("args").cloned().unwrap_or_default();
            let texto = args.get("texto").and_then(|x| x.as_str()).unwrap_or_default();
            if texto.trim().is_empty() {
                return reply(window, req, false, falha("argumento_invalido", "notificação sem texto"));
            }
            let titulo = args.get("titulo").and_then(|x| x.as_str()).unwrap_or("ShvIA");
            notify(window, &serde_json::json!({ "title": titulo, "body": texto }));
            reply(window, req, true, serde_json::json!({ "mostrada": true }));
        }
        "files.pick" => pick_files_titulo(
            window,
            req.to_string(),
            Some(titulo_do_seletor(&motivo)),
            Some(inicio + teto),
        ),
        "screen.snapshot" => {
            // The portal's parent window is read HERE, on the UI thread that owns the window;
            // the capture itself runs off it.
            #[cfg(target_os = "linux")]
            let pai = tela_linux::pai_do_portal(window);
            #[cfg(not(target_os = "linux"))]
            let pai = ();
            fora_da_ui(window, req, move |w| {
                if !confirmar_tela(w, &motivo, teto) {
                    return (false, falha("recusado", "a pessoa recusou a captura da tela — nada foi capturado"));
                }
                // The person may have taken minutes to answer a request the agent gave up on.
                if vencido(inicio.elapsed(), teto) {
                    return (false, falha("expirou", "o pedido expirou antes de a pessoa responder — nada foi capturado"));
                }
                let capturada = capturar_tela(pai);
                // Same check after: on GNOME the portal's own dialog waits for the person too. A
                // capture that finished late is dropped here and never leaves the desktop.
                if capturada.is_ok() && vencido(inicio.elapsed(), teto) {
                    return (false, falha("expirou", "o pedido expirou durante a captura — a imagem foi descartada"));
                }
                match capturada {
                    Ok(png) if png.len() > MAX_TELA_BYTES => {
                        (false, falha("grande_demais", "a captura passou de 10 MB e não foi enviada"))
                    }
                    Ok(png) => {
                        use base64::Engine;
                        (true, serde_json::json!({
                            "mimeType": "image/png",
                            "tamanho": png.len(),
                            "dataBase64": base64::engine::general_purpose::STANDARD.encode(&png),
                        }))
                    }
                    Err((codigo, msg)) => (false, falha(codigo, &msg)),
                }
            });
        }
        // The browser family (ADR-042): its own switch after the one above, then the same
        // `restante` ceiling, then everything it does is asked of the person one step at a time.
        c if navegador::COMANDOS.contains(&c) => {
            if !crate::tray::navegador_ligado(window.app_handle()) {
                return reply(window, req, false, falha("desligado",
                    "O navegador do agente está desligado. Ligue em “Navegador do agente nesta máquina”, no ícone do ShvIA na bandeja."));
            }
            let comando = c.to_string();
            let args = v.get("args").cloned().unwrap_or_default();
            fora_da_ui(window, req, move |w| navegador::executar(w, &comando, &motivo, &args, inicio, teto));
        }
        _ => reply(window, req, false, falha("comando_desconhecido", "comando de aparelho que este app não tem")),
    }
}

/// `aparelhoStatus` — whether the switch is on and which commands this build has. Reading it
/// changes nothing: it is how the page tells the person to turn the switch on, instead of
/// letting the agent fail without a reason.
pub(super) fn status(window: &WebviewWindow) -> serde_json::Value {
    let navegador = crate::tray::navegador_ligado(window.app_handle());
    serde_json::json!({
        "ligado": crate::tray::aparelho_ligado(window.app_handle()),
        // The browser family has its own switch; its commands are listed only while it is on,
        // and the flag lets the page say which switch to turn.
        "navegador": navegador,
        "comandos": comandos_ativos(navegador),
    })
}

/// The commands this build answers right now: the four device commands, plus the browser's
/// five while its switch is on.
fn comandos_ativos(navegador: bool) -> Vec<&'static str> {
    let mut v = COMANDOS.to_vec();
    if navegador {
        v.extend(navegador::COMANDOS);
    }
    v
}

pub(super) fn falha(codigo: &str, msg: &str) -> serde_json::Value {
    serde_json::json!({ "erro": msg, "codigo": codigo })
}

/// What `device.info` tells the agent. Deliberately little: the OS, the architecture, the app's
/// version and, on Linux, the session type — what changes how the other commands behave. No
/// host name, no user name, no paths: those identify a person, and the agent does not need them.
fn info(navegador: bool) -> serde_json::Value {
    // Only Linux adds a field; elsewhere `mut` is unused, which `-D warnings` refuses (measured
    // by the Windows cross-check, which Linux's own clippy cannot see).
    #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
    let mut out = serde_json::json!({
        "so": std::env::consts::OS,
        "arquitetura": std::env::consts::ARCH,
        "versaoDoApp": env!("CARGO_PKG_VERSION"),
        "comandos": comandos_ativos(navegador),
    });
    #[cfg(target_os = "linux")]
    {
        out["sessao"] = serde_json::json!(if crate::rapida::sessao_wayland() { "wayland" } else { "x11" });
    }
    out
}

/// The agent's reason as it will be shown: one line, no control characters, at most 300
/// characters. It is the agent's text, so it reaches the person framed as such, never as ours.
pub(super) fn motivo_limpo(s: &str) -> String {
    let uma_linha: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let junto = uma_linha.split_whitespace().collect::<Vec<_>>().join(" ");
    junto.chars().take(300).collect()
}

fn titulo_do_seletor(motivo: &str) -> String {
    if motivo.is_empty() {
        "O agente do ShvIA pediu arquivos".to_string()
    } else {
        format!("O agente do ShvIA pediu arquivos: {motivo}")
    }
}

/// Native confirmation before EVERY capture. Off the UI thread: `blocking_show` would deadlock
/// the event loop that has to paint the dialog.
fn confirmar_tela(window: &WebviewWindow, motivo: &str, teto: std::time::Duration) -> bool {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    window
        .app_handle()
        .dialog()
        .message(texto_da_confirmacao_da_tela(motivo, teto))
        .title("Captura da tela")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom("Capturar a tela".into(), "Recusar".into()))
        .blocking_show()
}

/// The dialog's text, apart so it can be tested. The reason is quoted and called the agent's,
/// because the agent wrote it: a reason that read as the app's own words would let a prompt
/// injection speak with the app's voice ("é seguro, clique em Capturar").
pub(super) fn texto_da_confirmacao_da_tela(motivo: &str, teto: std::time::Duration) -> String {
    let motivo = if motivo.is_empty() { "nenhum".to_string() } else { format!("“{motivo}”") };
    format!(
        "O agente do ShvIA pediu uma captura da sua tela inteira.\n\n\
         Motivo, nas palavras do agente (não verificado): {motivo}\n\n\
         A imagem vai para a conversa, no servidor do ShvIA. Feche ou esconda o que não deve \
         aparecer antes de continuar, e continue só se você pediu algo que precise da tela.\n\n\
         O pedido vale por {prazo}: depois disso, nada é capturado.",
        prazo = prazo_por_extenso(teto)
    )
}

/// The screen as PNG bytes, by the platform's own mechanism, with no capture library: a
/// library would have brought PipeWire and xcb to Linux and a second `windows` crate to
/// Windows for one command (measured: `xcap` 0.9.8).
#[cfg(target_os = "linux")]
fn capturar_tela(pai: tela_linux::Pai) -> Result<Vec<u8>, (&'static str, String)> {
    tela_linux::capturar(PRAZO_TELA, pai)
}

#[cfg(not(target_os = "linux"))]
fn capturar_tela(_: ()) -> Result<Vec<u8>, (&'static str, String)> {
    #[cfg(target_os = "macos")]
    {
        capturar_por_arquivo(|destino| {
            let mut c = crate::processo::comando("/usr/sbin/screencapture");
            // -x: no shutter sound. -t png. Without the Screen Recording permission, macOS
            // asks the first time; denied, the image holds only the wallpaper and menu bar.
            c.args(["-x", "-t", "png"]).arg(destino);
            c
        })
    }
    #[cfg(target_os = "windows")]
    {
        capturar_por_arquivo(|destino| {
            let mut c = crate::processo::comando("powershell.exe");
            c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command"])
                .arg(script_windows(destino));
            c
        })
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Err(("indisponivel", "captura de tela não existe nesta plataforma".to_string()))
    }
}

/// Runs a capture tool that writes a file, reads the file and removes it. The file lives in the
/// temporary folder under a random name and never outlives the call.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn capturar_por_arquivo(
    montar: impl FnOnce(&std::path::Path) -> Command,
) -> Result<Vec<u8>, (&'static str, String)> {
    let destino = std::env::temp_dir().join(format!("shvia-tela-{}.png", uuid::Uuid::new_v4().simple()));
    let saida = saida_com_prazo(montar(&destino), PRAZO_TELA);
    let lido = std::fs::read(&destino);
    let _ = std::fs::remove_file(&destino);
    match saida {
        Err(e) => Err(("falhou", format!("a captura não terminou: {e}"))),
        Ok(o) if !o.status.success() => Err((
            "falhou",
            format!("a captura falhou: {}", String::from_utf8_lossy(&o.stderr).trim()),
        )),
        Ok(_) => lido
            .ok()
            .filter(|b| !b.is_empty())
            .ok_or(("falhou", "a captura não produziu imagem".to_string())),
    }
}

/// The whole virtual screen (every monitor) to a PNG, with .NET's own `CopyFromScreen`.
#[cfg(target_os = "windows")]
fn script_windows(destino: &std::path::Path) -> String {
    let caminho = destino.to_string_lossy().replace('\'', "''");
    format!(
        "Add-Type -AssemblyName System.Windows.Forms,System.Drawing; \
         $r = [System.Windows.Forms.SystemInformation]::VirtualScreen; \
         $b = New-Object System.Drawing.Bitmap $r.Width, $r.Height; \
         $g = [System.Drawing.Graphics]::FromImage($b); \
         $g.CopyFromScreen($r.Left, $r.Top, 0, 0, $b.Size); \
         $b.Save('{caminho}', [System.Drawing.Imaging.ImageFormat]::Png); \
         $g.Dispose(); $b.Dispose()"
    )
}

/// Linux: the XDG desktop portal (`org.freedesktop.portal.Screenshot`), which works on X11 and
/// Wayland alike and is what GNOME and KDE route screenshots through. The desktop shows its own
/// permission dialog the first time; ours comes before it, every time.
#[cfg(target_os = "linux")]
mod tela_linux {
    use std::collections::HashMap;
    use zbus::blocking::{Connection, MessageIterator, Proxy};
    use zbus::zvariant::{OwnedValue, Value};

    /// How the portal is asked, decided from the asking window.
    ///
    /// - **X11**: non-interactive, with the window as `x11:<id>`. GNOME then shows its own
    ///   "share this screenshot" dialog over ours. Measured on GNOME 48: with an empty parent
    ///   it fails at once (response 2); with the window's id the dialog appears and waits.
    /// - **Wayland** (or no handle): interactive. A Wayland parent needs `xdg-foreign`, which
    ///   the app does not have; GNOME's interactive path calls the Shell directly, needs no
    ///   parent, and lets the person pick the area. ⚠️ Not measured: this machine runs X11.
    #[derive(Clone, Debug, PartialEq)]
    pub(crate) struct Pai {
        pub janela: String,
        pub interativo: bool,
    }

    pub(super) fn pai_do_portal(window: &tauri::WebviewWindow) -> Pai {
        use raw_window_handle::HasWindowHandle;
        let bruto = window.window_handle().ok().map(|h| h.as_raw());
        pai_de(bruto)
    }

    pub(super) fn pai_de(bruto: Option<raw_window_handle::RawWindowHandle>) -> Pai {
        use raw_window_handle::RawWindowHandle;
        match bruto {
            Some(RawWindowHandle::Xlib(h)) => Pai { janela: format!("x11:{:x}", h.window), interativo: false },
            Some(RawWindowHandle::Xcb(h)) => Pai { janela: format!("x11:{:x}", h.window.get()), interativo: false },
            _ => Pai { janela: String::new(), interativo: true },
        }
    }

    pub(super) fn capturar(prazo: std::time::Duration, pai: Pai) -> Result<Vec<u8>, (&'static str, String)> {
        let uri = pedir_ao_portal(prazo, &pai)?;
        let caminho = caminho_do_uri(&uri)
            .ok_or(("falhou", format!("o portal devolveu um endereço que não é arquivo: {uri}")))?;
        let bytes = std::fs::read(&caminho).map_err(|e| ("falhou", format!("não consegui ler a captura: {e}")))?;
        // The portal saved the file because we asked; the person did not. It goes once read.
        // Measured on GNOME 48 (01/10/2026): `~/Pictures/Screenshot.png`, a fixed name in the
        // person's own folder — left behind, every capture would pile up or overwrite there.
        let _ = std::fs::remove_file(&caminho);
        if bytes.is_empty() {
            return Err(("falhou", "a captura veio vazia".to_string()));
        }
        Ok(bytes)
    }

    fn erro_dbus(e: zbus::Error) -> (&'static str, String) {
        ("indisponivel", format!("o portal de captura não respondeu: {e}"))
    }

    fn pedir_ao_portal(prazo: std::time::Duration, pai: &Pai) -> Result<String, (&'static str, String)> {
        let conn = Connection::session().map_err(erro_dbus)?;
        let token = format!("shvia{}", uuid::Uuid::new_v4().simple());
        let remetente = conn
            .unique_name()
            .map(|n| n.as_str().trim_start_matches(':').replace('.', "_"))
            .ok_or(("indisponivel", "sem nome na sessão D-Bus".to_string()))?;
        // The request's path is known before the call (portal spec), so the reply is listened
        // for FIRST: a response that arrived between the call and the subscription would be lost.
        let caminho = format!("/org/freedesktop/portal/desktop/request/{remetente}/{token}");
        let regra = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.freedesktop.portal.Request")
            .and_then(|r| r.member("Response"))
            .and_then(|r| r.path(caminho.clone()))
            .map_err(erro_dbus)?
            .build();
        let mut sinais = MessageIterator::for_match_rule(regra, &conn, Some(4)).map_err(erro_dbus)?;

        let portal = Proxy::new(
            &conn,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Screenshot",
        )
        .map_err(erro_dbus)?;
        let mut opcoes: HashMap<&str, Value> = HashMap::new();
        opcoes.insert("handle_token", Value::from(token.as_str()));
        opcoes.insert("interactive", Value::from(pai.interativo));
        let _: zbus::zvariant::OwnedObjectPath =
            portal.call("Screenshot", &(pai.janela.as_str(), opcoes)).map_err(erro_dbus)?;

        // The wait has a deadline: the iterator blocks, so it runs on its own thread.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let r = sinais
                .next()
                .map(|m| m.map_err(|e| e.to_string()).and_then(|m| {
                    m.body()
                        .deserialize::<(u32, HashMap<String, OwnedValue>)>()
                        .map_err(|e| e.to_string())
                }));
            let _ = tx.send(r);
        });
        match rx.recv_timeout(prazo) {
            Err(_) => Err(("falhou", format!("o portal não respondeu em {} s", prazo.as_secs()))),
            Ok(None) => Err(("falhou", "a conexão com o portal fechou".to_string())),
            Ok(Some(Err(e))) => Err(("falhou", format!("resposta do portal ilegível: {e}"))),
            Ok(Some(Ok((codigo, resultados)))) => resposta(codigo, &resultados),
        }
    }

    /// The portal's verdict: 0 is success with a `uri`, 1 is the person refusing in the
    /// desktop's own dialog, anything else is a failure.
    pub(super) fn resposta(
        codigo: u32,
        resultados: &HashMap<String, OwnedValue>,
    ) -> Result<String, (&'static str, String)> {
        match codigo {
            0 => resultados
                .get("uri")
                .and_then(|v| String::try_from(v.clone()).ok())
                .ok_or(("falhou", "o portal respondeu sem a imagem".to_string())),
            1 => Err(("recusado", "a captura foi recusada no diálogo do sistema".to_string())),
            n => Err(("falhou", format!("o portal recusou a captura (código {n})"))),
        }
    }

    /// `file://` URI → path, percent-decoded. Anything else is refused: the portal answers with a
    /// local file, and reading some other scheme is not what this command is for.
    pub(super) fn caminho_do_uri(uri: &str) -> Option<std::path::PathBuf> {
        let resto = uri.strip_prefix("file://")?;
        let bytes = resto.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' {
                // A truncated escape is a malformed URI, not a literal `%`.
                let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        let caminho = String::from_utf8(out).ok()?;
        caminho.starts_with('/').then(|| std::path::PathBuf::from(caminho))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn uri_de_arquivo_vira_caminho_decodificado() {
            assert_eq!(
                caminho_do_uri("file:///home/a/Imagens/Captura%20de%20tela.png"),
                Some(std::path::PathBuf::from("/home/a/Imagens/Captura de tela.png"))
            );
            assert_eq!(caminho_do_uri("file:///tmp/x.png"), Some(std::path::PathBuf::from("/tmp/x.png")));
        }

        #[test]
        fn uri_que_nao_e_arquivo_local_e_recusado() {
            assert_eq!(caminho_do_uri("https://example.com/x.png"), None);
            assert_eq!(caminho_do_uri("file://relativo.png"), None);
            assert_eq!(caminho_do_uri("file:///quebrado%2"), None);
        }

        #[test]
        fn resposta_do_portal_distingue_recusa_de_falha() {
            let vazio = HashMap::new();
            assert_eq!(resposta(1, &vazio).unwrap_err().0, "recusado");
            assert_eq!(resposta(2, &vazio).unwrap_err().0, "falhou");
            assert_eq!(resposta(0, &vazio).unwrap_err().0, "falhou", "sucesso sem uri não é imagem");
            let mut com_uri = HashMap::new();
            com_uri.insert("uri".to_string(), OwnedValue::try_from(Value::from("file:///tmp/a.png")).unwrap());
            assert_eq!(resposta(0, &com_uri).unwrap(), "file:///tmp/a.png");
        }

        /// The real portal, on a real desktop session. Ignored by default: it captures this
        /// machine's screen. Run by hand with `cargo test tela_real -- --ignored --nocapture`.
        /// 🔴 X11 gets the window as parent and the non-interactive path; anything else asks
        /// interactively. An empty parent on X11 is the case that failed at once on GNOME 48.
        #[test]
        fn x11_passa_a_janela_e_wayland_pede_interativo() {
            use raw_window_handle::{RawWindowHandle, XlibWindowHandle};
            let x = pai_de(Some(RawWindowHandle::Xlib(XlibWindowHandle::new(0x320003e))));
            assert_eq!(x, Pai { janela: "x11:320003e".into(), interativo: false });
            let sem = pai_de(None);
            assert_eq!(sem, Pai { janela: String::new(), interativo: true });
        }

        /// The real portal, on a real desktop session. Ignored by default: it captures this
        /// machine's screen, and GNOME asks the person to share it. Run by hand with
        /// `SHVIA_SONDA_JANELA=x11:<id> cargo test tela_real -- --ignored --nocapture`.
        #[test]
        #[ignore]
        fn tela_real_pelo_portal() {
            let janela = std::env::var("SHVIA_SONDA_JANELA").unwrap_or_default();
            let pai = Pai { interativo: janela.is_empty(), janela };
            let png = capturar(std::time::Duration::from_secs(60), pai).expect("captura pelo portal");
            assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "não é PNG");
            eprintln!("captura: {} bytes", png.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motivo_vira_uma_linha_curta_sem_controle() {
        assert_eq!(motivo_limpo("ver\no erro\t na  tela"), "ver o erro na tela");
        assert_eq!(motivo_limpo(&"x".repeat(1000)).chars().count(), 300);
        assert_eq!(motivo_limpo("  "), "");
    }

    /// 🔴 The reason is the AGENT's text and the dialog says so, quoted. If it were shown bare,
    /// a prompt injection could write the dialog's argument in the app's voice.
    #[test]
    fn dialogo_da_tela_atribui_o_motivo_ao_agente() {
        let t = texto_da_confirmacao_da_tela("É seguro, clique em Capturar", PRAZO_DO_PEDIDO);
        assert!(t.contains("nas palavras do agente (não verificado): “É seguro, clique em Capturar”"), "{t}");
        assert!(t.contains("tela inteira"));
        assert!(t.contains("servidor do ShvIA"), "a pessoa precisa saber para onde a imagem vai");
        assert!(texto_da_confirmacao_da_tela("", PRAZO_DO_PEDIDO).contains("(não verificado): nenhum"));
    }

    #[test]
    fn seletor_diz_que_o_pedido_e_do_agente() {
        assert_eq!(titulo_do_seletor(""), "O agente do ShvIA pediu arquivos");
        assert_eq!(titulo_do_seletor("o log do erro"), "O agente do ShvIA pediu arquivos: o log do erro");
    }

    /// `device.info` gives away nothing that identifies the person.
    #[test]
    fn info_nao_carrega_nome_de_maquina_nem_de_pessoa() {
        let i = info(false);
        let texto = i.to_string();
        for campo in ["so", "arquitetura", "versaoDoApp", "comandos"] {
            assert!(i.get(campo).is_some(), "{campo} ausente: {texto}");
        }
        for vazamento in ["hostname", "host", "usuario", "user", "home", "caminho"] {
            assert!(i.get(vazamento).is_none(), "{vazamento} vazou: {texto}");
        }
        if let Ok(h) = std::env::var("HOME") {
            assert!(!texto.contains(&h), "a pasta pessoal vazou: {texto}");
        }
    }

    /// 🔴 The ceiling sits BELOW the server's `prazo_s` (180 s for the files and the capture), or it
    /// would protect nothing: the click of a request the agent already gave up on must do nothing.
    #[test]
    fn o_prazo_do_pedido_fica_abaixo_do_do_servidor() {
        const PRAZO_DO_SERVIDOR: std::time::Duration = std::time::Duration::from_secs(180);
        assert!(PRAZO_DO_PEDIDO < PRAZO_DO_SERVIDOR, "the desktop must stop acting before the server stops waiting");
        assert!(PRAZO_DO_PEDIDO >= std::time::Duration::from_secs(120), "a ceiling this short refuses people who are reading two dialogs");
    }

    #[test]
    fn o_limite_do_pedido_vale_na_fronteira() {
        use std::time::Duration;
        assert!(!vencido(Duration::ZERO, PRAZO_DO_PEDIDO));
        assert!(!vencido(PRAZO_DO_PEDIDO, PRAZO_DO_PEDIDO), "exactly at the ceiling still acts: past it is what expires");
        assert!(vencido(PRAZO_DO_PEDIDO + Duration::from_millis(1), PRAZO_DO_PEDIDO));
        // The same age against the request's OWN, shorter ceiling (the page had less left).
        assert!(vencido(Duration::from_secs(61), Duration::from_secs(60)));
        assert!(!vencido(Duration::from_secs(60), Duration::from_secs(60)));
    }

    #[test]
    fn o_dialogo_da_tela_avisa_quanto_tempo_o_pedido_vale() {
        use std::time::Duration;
        assert!(texto_da_confirmacao_da_tela("ver o erro", PRAZO_DO_PEDIDO).contains("vale por cerca de 3 minutos"));
        assert!(texto_da_confirmacao_da_tela("ver o erro", Duration::from_secs(45)).contains("vale por 45 segundos"));
        assert!(texto_da_confirmacao_da_tela("ver o erro", Duration::from_secs(120)).contains("vale por cerca de 2 minutos"));
    }

    /// The page's `restante` (SHVIA-WEB #454): the smaller of it and our own 170 s, counted from the
    /// bridge; 0 or less is already out; absent or not an integer falls back to 170 s, never longer.
    #[test]
    fn o_teto_e_o_menor_entre_o_restante_da_pagina_e_os_170_s() {
        use serde_json::json;
        use std::time::Duration;
        let teto = |v: serde_json::Value| teto_do_pedido(Some(&v));
        assert_eq!(teto(json!(170)), Some(PRAZO_DO_PEDIDO), "a fresh request: what the page sends with a 180 s server");
        assert_eq!(teto(json!(60)), Some(Duration::from_secs(60)), "the page has less left than our ceiling");
        assert_eq!(teto(json!(1)), Some(Duration::from_secs(1)));
        assert_eq!(teto(json!(9999)), Some(PRAZO_DO_PEDIDO), "a longer wait than our own is never taken");
        assert_eq!(teto_do_pedido(None), Some(PRAZO_DO_PEDIDO), "absent: a page that does not send it");
    }

    #[test]
    fn restante_zero_ou_negativo_ja_expirou_e_o_que_nao_e_inteiro_nao_alonga() {
        use serde_json::json;
        for ja_expirou in [json!(0), json!(-1), json!(-9999)] {
            assert_eq!(teto_do_pedido(Some(&ja_expirou)), None, "{ja_expirou} must refuse without opening anything");
        }
        // Not an integer: ignored, so the answer is our own ceiling — the stricter-or-equal side.
        for estranho in [json!("170"), json!(170.5), json!(null), json!(true), json!([170]), json!({"s": 5})] {
            assert_eq!(teto_do_pedido(Some(&estranho)), Some(PRAZO_DO_PEDIDO), "{estranho} lengthened or shortened the wait");
        }
    }

    /// SOURCE check, declared as such: the dispatch is driven by the WebView signal, which a unit
    /// test cannot reach. It holds the two gates in the arm's order — the switch is read BEFORE
    /// any command runs, and the capture asks BEFORE it captures.
    #[test]
    fn interruptor_vem_antes_de_qualquer_comando_e_o_dialogo_antes_da_captura() {
        let fonte = include_str!("aparelho.rs");
        let ini = fonte.find(&["pub(super) fn ", "aparelho(window"].concat()).expect("aparelho() moved");
        let corpo = &fonte[ini..];
        let fim = corpo.find("\npub(super) fn status").expect("status() moved");
        let corpo = &corpo[..fim];
        // The whole guard, not just the name: measured on 01/10/2026, `if false && !…` kept the
        // name in place, switched the gate off, and a check for the name alone stayed green.
        let guarda = [
            "    if !crate::tray::", "aparelho_ligado(window.app_handle()) {\n",
            "        return reply(window, req, false, falha(\"desligado\",",
        ]
        .concat();
        let interruptor = corpo.find(&guarda).expect("the switch guard is not the exact `if !…ligado { return … desligado`");
        let primeiro_comando = corpo.find(&["\"device", ".info\" =>"].concat()).expect("device.info arm missing");
        assert!(interruptor < primeiro_comando, "a command runs before the switch is read");
        let dialogo = corpo.find(&["confirmar", "_tela(w"].concat()).expect("capture dialog missing");
        let captura = corpo.find(&["capturar", "_tela(pai)"].concat()).expect("capture call missing");
        assert!(dialogo < captura, "the screen is captured before the person is asked");
        // The ceiling: checked after the dialog and BEFORE the capture, and again after it, so a
        // late image is dropped on the desktop. Counting the checks is what keeps either from being
        // deleted alone (the second one, `capturada.is_ok() && vencido(…)`, is easy to lose).
        // The WHOLE guard, `if <condition> {`, not the name inside it: measured on 02/10/2026, the
        // reversals `if false && vencido(…)` and `if false && limite.is_some_and(…)` kept the name
        // in place, switched the guard off, and a check for the name alone stayed green (as it
        // already had for the switch above).
        let guarda_antes = ["if ", "vencido(inicio.elapsed(), teto) {"].concat();
        let guarda_depois = ["if capturada.is_ok() && ", "vencido(inicio.elapsed(), teto) {"].concat();
        assert_eq!(corpo[dialogo..captura].matches(&guarda_antes).count(), 1,
            "the request is not checked between the dialog and the capture");
        assert_eq!(corpo[captura..].matches(&guarda_depois).count(), 1,
            "a capture that finished late would still be sent");
        assert!(corpo.contains(&["Some(inicio + ", "teto),"].concat()), "files.pick has no ceiling, or not the request's own");
        // `restante` of 0 or less answers `expirou` BEFORE any command opens anything: the exact
        // `let … else { return … }`, after the switch and before the dispatch.
        let guarda_restante = [
            "let Some(teto) = ", "teto_do_pedido(v.get(\"restante\")) else {\n",
            "        return reply(window, req, false, falha(\"expirou\"",
        ]
        .concat();
        let r = corpo.find(&guarda_restante).expect("the `restante` guard is not the exact `let Some(teto) … else { return … expirou`");
        let primeiro = corpo.find(&["\"device", ".info\" =>"].concat()).expect("device.info arm missing");
        assert!(r < primeiro, "a command can run before `restante` is read");
        let picker = include_str!("dialogos.rs");
        assert!(picker.contains(&["if limite.", "is_some_and(|l| std::time::Instant::now() > l) {"].concat()),
            "the picker reads files past the ceiling");
    }
}
