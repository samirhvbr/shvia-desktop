//! The page side of the bridge: the JS shim that `window.__shviaCode` is, and the per-session
//! capability token it carries (only pages of a `SERVER_HOSTS` host get it, so an embedded
//! cross-origin frame that reaches the native handler is ignored). Split out of
//! `code_bridge.rs` in 1.6.41.

use super::*;

/// Token de capacidade da sessão do app. É injetado SOMENTE nas páginas do host
/// canônico (um dos `SERVER_HOSTS`, via `on_page_load`) dentro do shim da ponte, e
/// TODA mensagem página→Rust precisa carregá-lo em `__t`.
///
/// Fecha o furo do handler nativo ser alcançável por QUALQUER frame: o
/// `messageHandler`/`window.chrome.webview` existe para todos os frames da
/// webview, então um `<iframe>` cross-origin embutido na página poderia postar
/// `spawn`/`listTree`/`gitStatus` direto. Esse iframe NÃO consegue ler o token
/// (não injetado nele + closure do frame pai é cross-origin), então suas
/// mensagens são descartadas em `handle_message`. Uniforme nos 3 SOs — não
/// depende de API de origem de frame (que o webkit2gtk 2.0 não expõe).
pub(super) static BRIDGE_TOKEN: OnceLock<String> = OnceLock::new();

/// Token da sessão (gerado uma vez por processo). 32 hex, seguro em JS/JSON.
pub fn bridge_token() -> &'static str {
    BRIDGE_TOKEN
        .get_or_init(|| uuid::Uuid::new_v4().simple().to_string())
        .as_str()
}

/// Interpola o token de sessão no placeholder `__SHVIA_BRIDGE_TOKEN__` de um
/// script injetado (BRIDGE_JS e PRICE_ALERT_NOTIFY_JS). Chamado no `on_page_load`.
pub fn inject_token(js: &str) -> String {
    js.replace("__SHVIA_BRIDGE_TOKEN__", bridge_token())
}

/// O shim injetado em cada página (`on_page_load`). Define `window.__shviaCode`
/// (a API que a UI do Modo Code no SHVIA-WEB chama) e `window.__shviaDesktop`
/// (flag p/ o web mostrar o Modo Code SÓ onde a ponte existe). Autocontido, ES5,
/// self-guard: sem o handler nativo, não faz nada.
pub const BRIDGE_JS: &str = r#"(function () {
  if (window.__shviaCode) return;
  // Transporte página→Rust por WebView, escolhido por SO:
  //  - WebKit (Linux/macOS): window.webkit.messageHandlers.shviaCode.postMessage(str)
  //  - WebView2 (Windows):   window.chrome.webview.postMessage(str)
  // Rust→página é sempre eval (_reply/_emit). Sem nenhum dos dois → sem Modo
  // Code (fail-safe: não define __shviaCode e o web esconde o toggle).
  var wk = window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.shviaCode;
  var w2 = window.chrome && window.chrome.webview;
  if (!wk && !w2) return;
  function sendNative(str) { if (wk) { wk.postMessage(str); } else { w2.postMessage(str); } }
  var reqs = {}, seq = 0, listeners = [];
  function post(action, data) {
    return new Promise(function (res, rej) {
      var id = 'r' + (++seq);
      reqs[id] = { res: res, rej: rej };
      var msg = { action: action, reqId: id };
      if (data) for (var k in data) msg[k] = data[k];
      msg.__t = '__SHVIA_BRIDGE_TOKEN__'; // token de capacidade (ver bridge_token)
      sendNative(JSON.stringify(msg));
    });
  }
  window.__shviaCode = {
    // sessão do agente
    spawn: function (o) { return post('spawn', o || {}); },   // erro de motor ausente vem {error, codigo} — ver claudeModels;  {projectDir, apiKey, model?, effort?, url?, engine?, modelDoClaude?, accountId?, autonomy?}  autonomy:{stopByHost, maxIterations, maxCostUsd} = a Run (RUN-20260910), só no motor claude;  engine:'claude' = assinatura; modelDoClaude:true = model/effort vieram de claudeModels(), nao do gateway; accountId = perfil de conta (ver claudeAccounts)
    send:  function (o, sessao) { return post('send', sessao ? { payload: o, sessao: sessao } : { payload: o }); }, // {type:'user',text} | {id,decision}; sessao = which session (recursos.sessoes)
    kill:  function (sessao) { return post('kill', sessao ? { sessao: sessao } : null); }, // sem sessao = a sessão única de antes da 1.8.0
    onEvent: function (cb) { if (typeof cb === 'function') listeners.push(cb); },
    // pasta / vínculo
    pickFolder: function () { return post('pickFolder'); },
    // Escolhe arquivos com o diálogo NATIVO, que abre na última pasta usada
    // (o <input type="file"> do WebView não deixa escolher a pasta inicial e
    // caía sempre nos favoritos — queixa de 03/08). Devolve os BYTES, porque
    // a página não tem acesso ao disco: {files:[{name, dataBase64, size}],
    // skipped:[str]}. Cancelar volta {files:[], canceled:true}.
    pickFiles: function () { return post('pickFiles'); },
    // Salva artefato gerado (imagem/código) com diálogo nativo do SO. A PÁGINA
    // manda os bytes — ela tem a sessão autenticada, o Rust não. {name, dataBase64}
    // → {saved:true, path} | {saved:false} quando o usuário cancela.
    saveFile: function (o) { return post('saveFile', o || {}); },
    // Item D3: grava a config de um cliente de CLI no host. {client, baseUrl, apiKey, model}
    // → {written:true, path, backup} | {written:false} quando o usuário cancela o diálogo.
    // `client` só aceita 'continue' | 'claude-code' | 'env' — os outros do gerador da web
    // não produzem arquivo.
    writeCliConfig: function (o) { return post('writeCliConfig', o || {}); },
    getBinding: function (pid) { return post('getBinding', { projectId: pid }); },
    setBinding: function (pid, path) { return post('setBinding', { projectId: pid, path: path }); },
    // painel da pasta (read-only, pelo app)
    gitStatus: function (path) { return post('gitStatus', { path: path }); },
    listTree: function (path) { return post('listTree', { path: path }); },
    // Diff de UM arquivo, para a aba "Alterações" do painel abrir ao clique.
    // → {ok:true, diff, truncated, staged} | {ok:false, erro}
    // `staged`: o arquivo pode estar no índice (git add) e aí o diff da árvore de
    // trabalho vem VAZIO — a página precisa saber que "vazio" ali significa
    // "já preparado", não "sem mudança".
    gitDiff: function (path, file) { return post('gitDiff', { path: path, file: file }); },
    // Prévia de UM arquivo, para a árvore da aba "Arquivos" abrir ao clique.
    // → {ok:true, content, truncated, binary, bytes} | {ok:false, erro}
    // `binary`: arquivo com NUL vem com content vazio — a página mostra o tamanho,
    // não os bytes. `truncated`: arquivo grande é cortado (é prévia, não editor).
    readFile: function (path, file) { return post('readFile', { path: path, file: file }); },
    // Catálogo do motor Claude Code, perguntado ao Agent SDK (não é o catálogo
    // do gateway). Cada item traz supportsEffort + supportedEffortLevels, para a
    // UI listar o que existe e DESABILITAR o que não se aplica.
    // → {modelos:[{value,resolvedModel,displayName,description,supportsEffort,supportedEffortLevels}]} | {erro, codigo?}
    // `codigo` ('runner_ausente'|'codex_ausente') existe para a tela escolher a LÍNGUA: o `erro`
    // é português e a tela é bilíngue. Sem código conhecido, mostre o `erro` — é a reserva.
    //
    // `{accountId}` escolhe SOB QUAL CONTA o catálogo é perguntado (ADR-033). Sem ele, a
    // conta padrão do CLI. O catálogo é por conta: assinaturas diferentes oferecem modelos
    // diferentes, e uma lista carregada sob outra conta é um alvo que o turno vai recusar.
    codexModels: function () { return post('codexModels', {}); },
    claudeModels: function (o) { return post('claudeModels', o || {}); },
    // Perfis de conta do Claude Code desta máquina (ADR-033).
    // → {contas:[{id,rotulo,disponivel,motivo}], selecionada}
    //
    // `disponivel` é o diretório de configuração EXISTIR — não é promessa de login válido,
    // e o rótulo é palavra do usuário, não identidade verificada de organização. A página
    // manda um `id` da lista, NUNCA um caminho: quem traduz id→diretório é o Rust, e é essa
    // a mesma linha do ADR-026 (a página propõe valores) e do ADR-031 (a cerca vem do gesto).
    claudeAccounts: function () { return post('claudeAccounts'); },
    // Pergunta ao SHELL quais funções trocam de conta. Gesto explícito (botão), nunca no
    // boot: sobe um shell interativo. → {candidatos:[{alias,var,dir,disponivel}]}
    // Instala o `claude-runner` a partir da fonte que veio NO INSTALADOR. Gesto
    // explícito: baixa pacote da rede e leva segundos. → {saida} | {erro, codigo}
    // codigo: 'recursos_ausentes' (build sem a fonte) | 'bash_ausente' | 'powershell_ausente' (Windows) | 'instalacao_falhou'
    // O `erro` de 'instalacao_falhou' é a saída INTEIRA do install.sh — mostre como veio.
    // Portão 2 — o login pela tela. `status` é leitura pura e sem cota.
    // → {loggedIn, email, subscriptionType, proveniencia, proveniencia_motivo?} | {erro, codigo}
    //
    // 🔴 `proveniencia` diz se dá para AFIRMAR que a resposta veio do perfil pedido:
    //   'confirmada'      → vale o `loggedIn`
    //   'nao_confirmavel' → TERCEIRO ESTADO ("não consegui perguntar"), NUNCA "não conectado"
    //
    // Perfil de credencial é sempre 'nao_confirmavel', e não é defeito do cliente: a
    // variável dele move só a chave, então o `configDirectory` volta sendo a casa padrão
    // por desenho. Renderizar isso como "não conectado" manda refazer um login de pé.
    claudeAuthStatus: function (o) { return post('claudeAuthStatus', o || {}); },
    // `Start` devolve a URL e DEIXA o cliente vivo esperando o código; `Code` o entrega.
    // Um por vez: começar de novo cancela o anterior (o código só vale para o PKCE que o gerou).
    claudeAuthLoginStart: function (o) { return post('claudeAuthLoginStart', o || {}); },  // → {url, login} | {erro, codigo}; codigo 'cancelado' = the person said no in the native dialog (1.6.38)
    // 🔴 Volta na HORA, e NÃO diz se o login deu certo. O fim chega pelo evento
    // `{type:'claude_login_fim', code}`, e o veredito vem de um `claudeAuthStatus` depois —
    // nunca do stdout (o cliente não promete o que imprime) nem do código de saída, que é
    // `0` nos DOIS finais (medido). E há um segundo final: se a pessoa autorizar na aba do
    // navegador, o processo conclui sem código nenhum e o evento chega sem ninguém ter
    // colado nada.
    claudeAuthLoginCode: function (o) { return post('claudeAuthLoginCode', o || {}); },    // {codigo} → {entregue} | {erro, codigo}
    claudeAuthLoginCancel: function () { return post('claudeAuthLoginCancel'); },
    claudeRunnerInstall: function () { return post('claudeRunnerInstall'); },
    claudeAccountsDetect: function () { return post('claudeAccountsDetect'); },
    // Cadastra um candidato pelo ALIAS — a página nunca manda caminho. {alias, rotulo?}
    claudeAccountAdd: function (o) { return post('claudeAccountAdd', o || {}); },
    // Persiste a conta escolhida no dispositivo. → {selecionada} | {erro, codigo}
    // Recusa id fora da lista: escolher não pode ser o jeito de inventar um perfil.
    claudeAccountSelect: function (id) { return post('claudeAccountSelect', { accountId: id }); },
    // Estado do motor no disco: {found, bundled, version, path}. `found` é o
    // binário EXISTIR e `version` é ele RESPONDER `--version` — separados de
    // propósito, porque "está lá e não roda" é diagnóstico diferente de "não está
    // lá". `engine`: 'gateway' (anna) | 'claude' (claude-runner).
    //
    // ⚠️ O Rust tratava esta ação desde a 1.0.0 e o wrapper NUNCA a expôs — braço
    // implementado e inalcançável, com um comentário ao lado dizendo que "a página
    // pergunta antes de oferecer o Modo Code". Nenhuma página perguntava.
    //
    // Ela NÃO serve para decidir se a imagem é oferecida: para isso existe
    // `recursos` abaixo, e a diferença importa. Este `post` só existe em cascas
    // que já o expõem — as mesmas que já trazem os motores novos —, então usá-lo
    // como gate responderia sempre "sim". Aqui ele serve para MOSTRAR a versão do
    // motor a quem for pedir suporte.
    engineStatus: function (engine) { return post('engineStatus', { engine: engine || 'gateway' }); },
    // Native OS notification, fire-and-forget (no reqId, no reply): the same `notify`
    // action the shell's own notification poll posts (ADR-011), now reachable from the
    // page — the Run's gate card uses it when the person walked away (RUN-20260910, B5).
    // It exists here because every message without the token is dropped in Rust, so a
    // page posting to the native handler by hand never reached the shell (and is banned
    // on the web side by `prova-voz-passa-pelo-token`, finding F-16). Presence of this
    // method is the capability flag, same design as `recursos`.
    notify: function (o) {
      o = o || {};
      sendNative(JSON.stringify({ action: 'notify', title: String(o.title || ''), body: String(o.body || ''), __t: '__SHVIA_BRIDGE_TOKEN__' }));
    },
    // CAPACIDADES desta casca. Constante local, sem ida ao Rust — a pergunta é
    // "esta versão do app sabe fazer X?", e a resposta está na própria casca.
    //
    // ⚠️ Por que existe. A página do Modo Code vem do SERVIDOR e atualiza a cada
    // deploy; a casca e os motores só mudam quando alguém INSTALA. E os motores nem
    // chegam pelo mesmo caminho: o `anna` viaja embutido no app (`externalBin` em
    // `tauri.conf.json`, item D5), o `claude-runner` NÃO — é instalação separada e
    // opcional, por `claude-runner/install.sh` (deixa um wrapper em `~/.local/bin`).
    // Medido em 08/09: o bundle instalado traz `anna` e `shvia-desktop`, e nada mais.
    // São três ritmos, então uma página nova conversando com uma casca velha é o
    // estado normal, não a exceção. Sem este flag, a página mandaria
    // `images` no payload e a casca velha — que só lê `text` — descartaria a
    // figura em SILÊNCIO: o chip na tela dizendo que foi, o modelo respondendo
    // sem ter visto nada. Ausência do flag é a resposta "não", e é por isso que
    // ele testa presença em vez de comparar número de versão.
    //
    //   imagem: os DOIS motores carregam imagem no turno (anna >= 0.11.4 pelo
    //           campo `images`; claude-runner por blocos do Agent SDK).
    //   conta:  esta casca sabe escolher perfil de conta do Claude Code (ADR-033). Sem o
    //           flag, o web não desenha o seletor CONTA e a régua fica a de antes — que é
    //           o certo, porque um seletor que a casca ignorasse mostraria a conta errada
    //           na tela enquanto o turno rodasse na outra.
    //   run:    this shell carries the Run to the Claude runner (RUN-20260910, B3): the
    //           `autonomy` field of `spawn` becomes `--parada host` and the two caps. Without
    //           the flag the page falls back to continuing BETWEEN turns only, which works on
    //           every shell — presence, never a version number, same as `imagem`.
    //   sessoes: several agent sessions per window (1.8.0). `spawn({..., sessao})` names one,
    //           `send(o, sessao)` and `kill(sessao)` address it, and every event it emits
    //           carries `evt.sessao`. Switching project no longer has to kill the agent that
    //           is working: each project keeps its own. Without a name everything is the one
    //           session of before, so a page that never reads this flag sees no change.
    recursos: { imagem: true, conta: true, run: true, sessoes: true },
    // chamados pelo Rust (eval):
    _reply: function (id, ok, data) { var r = reqs[id]; if (r) { delete reqs[id]; ok ? r.res(data) : r.rej(data); } },
    _emit: function (evt, sessao) { if (sessao && evt && typeof evt === 'object') evt.sessao = sessao; for (var i = 0; i < listeners.length; i++) { try { listeners[i](evt); } catch (e) {} } }
  };
  window.__shviaDesktop = wk ? { platform: 'webkit', bridge: 'webkit' }
                             : { platform: 'windows', bridge: 'webview2' };
})();"#;
