// Rulers for the translation layer. They are the only part of this runner that can
// be tested without a live Codex — `codex-runner.mjs` spawns a child and owns
// stdio, so it runs on import. Same split, and same reason, as `politica.mjs` on
// the Claude side.
import { strict as assert } from "node:assert";
import { test } from "node:test";
import {
  PEDIDOS_QUE_BLOQUEIAM,
  decisaoParaResposta,
  ehDecisao,
  pedidoParaGate,
  POLITICA,
  politicaValida,
  traduzirNotificacao,
} from "./protocolo.mjs";

test("a política é UMA, e é a que a medição M provou", () => {
  // Nao ha seletor de nivel no motor Codex. Manual/Edit/Auto seriam tres rotulos
  // sobre um comportamento so — a pill mentindo de tres jeitos.
  assert.equal(POLITICA.askForApproval, "on-request");
  assert.equal(POLITICA.sandboxMode, "workspace-write");
});

test("`never` e `danger-full-access` nunca são política válida", () => {
  // 🔴 A regua central. `never` faz o Codex nao perguntar NEM na fronteira, e
  // `danger-full-access` tira o sandbox — os dois transformam este motor num modo
  // sem cerca com a pill ainda dizendo "fronteira com cartão". Se alguem
  // reintroduzir seletor de nivel, e aqui que morde.
  assert.equal(politicaValida(POLITICA), true);
  assert.equal(politicaValida({ askForApproval: "never", sandboxMode: "workspace-write" }), false);
  assert.equal(politicaValida({ askForApproval: "on-request", sandboxMode: "danger-full-access" }), false);
  assert.equal(politicaValida(null), false);
});

test("a decisão usa o vocabulário do pedido, que é diferente entre v1 e v2", () => {
  // v1 responde ReviewDecision; v2 responde accept/decline. Mandar o vocabulário
  // errado é recusado pelo servidor, e o turno fica PENDURADO — a falha parece
  // travamento, não erro, e é por isso que isto tem teste.
  assert.deepEqual(decisaoParaResposta("execCommandApproval", "approve"), { decision: "approved" });
  assert.deepEqual(decisaoParaResposta("execCommandApproval", "always"), { decision: "approved_for_session" });
  assert.deepEqual(decisaoParaResposta("execCommandApproval", "reject"), { decision: "denied" });

  const v2 = "item/commandExecution/requestApproval";
  assert.deepEqual(decisaoParaResposta(v2, "approve"), { decision: "accept" });
  assert.deepEqual(decisaoParaResposta(v2, "always"), { decision: "acceptForSession" });
  assert.deepEqual(decisaoParaResposta(v2, "reject"), { decision: "decline" });
});

test("decisão desconhecida NEGA", () => {
  // O default do desconhecido é o não. Um valor novo vindo do host não pode virar
  // aprovação por descuido de tabela.
  assert.deepEqual(decisaoParaResposta("item/fileChange/requestApproval", "talvez"), { decision: "decline" });
  assert.deepEqual(decisaoParaResposta("execCommandApproval", ""), { decision: "denied" });
});

test("pedido de comando vira cartão com o comando à vista", () => {
  const g = pedidoParaGate({
    id: 7,
    method: "item/commandExecution/requestApproval",
    params: { command: ["rm", "-rf", "build"], reason: "limpar artefatos" },
  });
  assert.equal(g.type, "gate_request");
  assert.equal(g.id, 7);
  assert.equal(g.scope, "Bash");
  assert.equal(g.preview.kind, "command");
  assert.equal(g.preview.command, "rm -rf build");
  assert.equal(g.preview.why, "limpar artefatos");
});

test("pedido de arquivo SEM patch não inventa diff", () => {
  // Os params da v2 não carregam o patch (ele veio antes, em notificação). Um
  // cartão mostrando um diff que o agente não propôs seria mentira com diff junto.
  const g = pedidoParaGate({
    id: "abc",
    method: "item/fileChange/requestApproval",
    params: { grantRoot: "/home/u/proj", reason: "editar fora do sandbox" },
  });
  assert.equal(g.scope, "Edit");
  assert.equal(g.preview.kind, "command");
  assert.match(g.preview.command, /apply file changes in \/home\/u\/proj/);
});

test("método que não bloqueia não vira cartão", () => {
  assert.equal(pedidoParaGate({ id: 1, method: "item/tool/call", params: {} }), null);
  assert.equal(pedidoParaGate(null), null);
  assert.ok(PEDIDOS_QUE_BLOQUEIAM.has("applyPatchApproval"));
});

test("a resposta do agente vira texto, e o item completo NÃO repete", () => {
  assert.deepEqual(
    traduzirNotificacao({ method: "item/agentMessage/delta", params: { delta: "oi" } }),
    { type: "text", delta: "oi" },
  );
  // Se `agentMessage` completo virasse texto, a resposta inteira apareceria uma
  // segunda vez embaixo da que foi streamada.
  assert.equal(
    traduzirNotificacao({ method: "item/completed", params: { item: { type: "agentMessage", text: "oi" } } }),
    null,
  );
});

test("uso vem do tokenUsage, em camelCase, e é o do TURNO", () => {
  // 🔴 Este teste nasceu de um defeito que o smoke ao vivo mostrou e nenhum teste
  // puro teria mostrado: a versão anterior lia `params.usage` no `turn/completed`,
  // que NÃO tem esse campo (os params são `{threadId, turn}`), e reportava
  // `tokens: 0` em todo turno. A conta estava certa; a forma é que era inventada.
  assert.equal(traduzirNotificacao({ method: "turn/completed", params: { turn: {} } }), null);

  const e = traduzirNotificacao({
    method: "thread/tokenUsage/updated",
    params: { tokenUsage: { last: { inputTokens: 100, outputTokens: 7, totalTokens: 107 } } },
  });
  assert.equal(e.type, "usage");
  assert.equal(e.tokens, 107);
  // O motor roda FORA do gateway: não há custo auditado a afirmar. `null` com
  // `estimated` é honesto; um número seria invenção.
  assert.equal(e.cost, null);
  assert.equal(e.estimated, true);

  // `last` é o turno; `total` é a thread inteira e subiria para sempre num
  // contador por turno.
  const soTotal = traduzirNotificacao({
    method: "thread/tokenUsage/updated",
    params: { tokenUsage: { last: { inputTokens: 2, outputTokens: 3 }, total: { totalTokens: 9999 } } },
  });
  assert.equal(soTotal.tokens, 5, "caiu no total da thread em vez do turno");
});

test("comando executado vira tool_call e tool_result", () => {
  const call = traduzirNotificacao({
    method: "item/started",
    params: { item: { id: "i1", type: "commandExecution", command: "ls" } },
  });
  assert.equal(call.type, "tool_call");
  assert.equal(call.name, "Bash");
  assert.equal(call.id, "i1");

  const res = traduzirNotificacao({
    method: "item/completed",
    params: { item: { id: "i1", type: "commandExecution", aggregatedOutput: "a\nb" } },
  });
  assert.equal(res.type, "tool_result");
  assert.equal(res.content, "a\nb");
  assert.equal(res.bytes, 3);
});

test("o desconhecido é ignorado, não vira ruído nem quebra", () => {
  // O app-server publica 68 notificações; a maioria é de superfície que o SHVIA
  // não tem. Reencaminhar tudo encheria a timeline; falhar no desconhecido faria
  // de toda release do Codex uma quebra aqui.
  for (const m of ["thread/realtime/started", "marketplace/updated", "fs/changed", undefined]) {
    assert.equal(traduzirNotificacao({ method: m, params: {} }), null);
  }
});

test("erro do servidor chega COM o texto, que vem aninhado", () => {
  // 🔴 O payload real, capturado em 09/09/2026. Lendo `params.message` isto virava
  // "unknown error" — a única cópia do que o servidor disse, descartada.
  const e = traduzirNotificacao({
    method: "error",
    params: {
      error: { message: "Reconnecting... 2/5", additionalDetails: "the model `x` does not exist" },
      willRetry: true,
    },
  });
  assert.match(e.message, /Reconnecting/);
  assert.match(e.message, /does not exist/, "perdeu o additionalDetails, que é a metade útil");
  // Retentativa NÃO é falha: vira aviso, senão a timeline declara morto um turno vivo.
  assert.equal(e.type, "warn");

  const fatal = traduzirNotificacao({ method: "error", params: { error: { message: "boom" } } });
  assert.deepEqual(fatal, { type: "error", message: "boom" });

  // Nunca "unknown error" mudo: sem texto, diz que não veio descrição.
  assert.match(traduzirNotificacao({ method: "error", params: {} }).message, /sem descrição/);
  assert.equal(
    traduzirNotificacao({ method: "configWarning", params: { summary: "projeto não confiável" } }).type,
    "warn",
  );
});

test("a decisão do cartão de id ZERO é aceita", () => {
  // 🔴 O app-server numera os pedidos DELE a partir de zero, então o primeiro
  // cartão de toda sessão chega com `id: 0`. A versão anterior testava
  // `msg.id && msg.decision` — zero é falsy, a decisão era descartada, o runner
  // nunca respondia e o turno pendurava para sempre. Congelamento sem erro, no
  // primeiro cartão que a pessoa veria neste motor.
  assert.equal(ehDecisao({ id: 0, decision: "reject" }), true, "id 0 foi descartado de novo");
  assert.equal(ehDecisao({ id: "abc", decision: "approve" }), true);

  // E o que NÃO é decisão continua fora.
  assert.equal(ehDecisao({ type: "user", text: "oi" }), false);
  assert.equal(ehDecisao({ type: "exit" }), false);
  assert.equal(ehDecisao({ id: 3 }), false, "sem decision não é decisão");
  assert.equal(ehDecisao(null), false);
});

// ── validação de payload contra o schema do próprio Codex ────────────────────
import { ESQUEMA_DO_METODO, validarPayload } from "./esquema.mjs";

test("campo inventado é RECUSADO — foi assim que o `sandboxMode` apareceu", () => {
  // 🔴 O caso real, na primeira execução do validador: este runner mandava
  // `sandboxMode` para o `thread/start` desde a primeira linha. O campo NÃO existe
  // (é `sandbox`), e o servidor o descartava em silêncio — a política que eu
  // achava estar definindo nunca foi definida, e o que eu media era o default do
  // Codex. Campo errado não falha: ele SOME.
  const p = validarPayload("thread/start", { cwd: "/x", sandboxMode: "workspace-write" });
  assert.equal(p.length, 1);
  assert.match(p[0], /sandboxMode.*não existe/);

  // E a forma certa passa.
  assert.deepEqual(validarPayload("thread/start", { cwd: "/x", sandbox: "workspace-write", approvalPolicy: "on-request" }), []);
});

test("campo obrigatório ausente é recusado", () => {
  assert.match(validarPayload("turn/start", { threadId: "t" })[0], /obrigatório `input`/);
  assert.deepEqual(validarPayload("turn/start", { threadId: "t", input: [] }), []);
});

test("tipo errado é recusado", () => {
  assert.match(validarPayload("command/exec", { command: "ls" })[0], /devia ser array/);
  assert.deepEqual(validarPayload("command/exec", { command: ["ls"] }), []);
});

test("método que não mandamos não é afirmado", () => {
  // Não inventa reprovação sobre o que este runner não envia — uma régua que opina
  // sobre o que não conhece vira ruído e some do radar de quem a lê.
  assert.deepEqual(validarPayload("thread/list", { qualquer: 1 }), []);
  assert.ok(Object.keys(ESQUEMA_DO_METODO).includes("initialize"));
});

// ── a lista de segredos é IMPORTADA, não copiada ─────────────────────────────
import { caminhoProibido } from "../claude-runner/politica.mjs";
import { readFileSync } from "node:fs";

test("o aviso de segredo usa a MESMA lista do outro motor", () => {
  // 🔴 A régua é sobre a fonte, não sobre o resultado. Copiar os padrões para cá
  // passaria em qualquer teste de comportamento e divergiria no dia em que alguém
  // acrescentasse um padrão de um lado só — e a cópia que ficasse para trás seguiria
  // verde sem proteger nada. Então o que se trava é o `import`.
  const fonte = readFileSync(new URL("./codex-runner.mjs", import.meta.url), "utf8");
  assert.match(fonte, /from "\.\.\/claude-runner\/politica\.mjs"/, "a lista deixou de ser importada");
  assert.ok(!/function caminhoProibido/.test(fonte), "alguém copiou a função para cá");

  // E a lista importada reconhece o que precisa reconhecer.
  for (const nome of [".env", ".env.local", "id_rsa", "chave.pem", ".git"]) {
    assert.equal(caminhoProibido(nome), true, `${nome} deixou de contar como segredo`);
  }
  // `.env.example` é o contraexemplo que a lista já trata — nunca foi segredo.
  assert.equal(caminhoProibido(".env.example"), false);
  assert.equal(caminhoProibido("README.md"), false);
});
