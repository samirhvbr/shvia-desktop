# Contas do Claude Code no Modo Code

> **Estado:** implementado na 1.4.16 (Linux); em macOS o seletor não oferece perfil nenhum
> — ver "macOS: a variável que separa as contas do dono é outra" e a proposta em
> [`.continue/contas-claude-macos.md`](../../.continue/contas-claude-macos.md).
> Decisão e alternativas em
> [ADR-033](../decisoes.md#adr-033--a-conta-do-claude-code-é-um-id-de-lista-fechada-e-o-diretório-vai-no-filho).
> Supersedes the proposal of 05/09/2026, archived beside this file as
> [CONTAS-CLAUDE-proposta-20260905.md](CONTAS-CLAUDE-proposta-20260905.md).

O motor **Claude Code (assinatura)** ([ADR-014](../decisoes.md#adr-014)) roda com o login
que o cliente oficial guardou. Quem tem mais de uma conta — uma da empresa, uma pessoal —
troca entre elas por `CLAUDE_CONFIG_DIR`. No terminal isso são dois aliases; no Modo Code
passou a ser o seletor **CONTA**, ao lado do MODELO.

---

## O que o usuário vê

Com o motor Claude Code ativo, a régua do compositor mostra **CONTA** no lugar do INFRA
(que ali é uma opção só, travada em "Anthropic (assinatura)" — ver ADR-033). Trocar de
conta **encerra a sessão do agente**: aparece a fronteira na timeline, o transcrito lido
continua na tela, e a próxima mensagem sobe um runner novo.

Conta cuja pasta de configuração não está mais no lugar aparece **desabilitada, com o
motivo**. O rótulo nunca afirma que a conta está conectada — ver "O que `disponivel` não
diz" abaixo.

---

## Os perfis, e como registrar um novo

O registro vive em `contas-claude.json`, no `app_config_dir` do app, ao lado do
`pastas-autorizadas.json` e do `modo-code-bindings.json`. Ele **não sobe para o servidor**
e não segue a pessoa para outra máquina: qual conta você usa neste computador não é fato
sobre a sua conta do ShvIA.

```json
{
  "contas": [
    { "id": "empresa-blue3", "rotulo": "Empresa · Blue3", "dir": "/home/samir/.claude-blue3" },
    { "id": "pessoal",       "rotulo": "Pessoal",         "dir": "/home/samir/.claude-pessoal" }
  ],
  "selecionada": "padrao"
}
```

**Semeadura, uma vez.** Na primeira execução o app grava os perfis conhecidos cujo
diretório **já existe** — `~/.claude-blue3` e `~/.claude-pessoal`. Nada é criado: uma pasta
vazia inventada por nós listaria como conta e falharia no primeiro turno.

**`padrao` é implícito** e não aparece no arquivo. Ele é a conta sem diretório: o processo
filho não recebe `CLAUDE_CONFIG_DIR` e o CLI usa o default dele. Apagá-lo do arquivo não o
remove da lista — sem ele não haveria caminho de volta ao comportamento de antes.

**Outra máquina, outro caminho:** acrescente a entrada à mão e reabra o app. As regras que
o arquivo tem de obedecer, conferidas a **cada leitura** (a regra pode ter endurecido desde
que alguém escreveu ali):

| campo | regra |
|---|---|
| `id` | minúsculas, dígitos e traço; começa com alfanumérico; até 32 caracteres |
| `dir` | caminho **absoluto**, dentro do seu `$HOME` |
| `rotulo` | não vazio |

Entrada que não passa é **descartada**, não corrigida — um palpite nosso sobre o que a
pessoa quis dizer rodaria um turno num diretório que ninguém escreveu.

⚠️ **O arquivo não guarda credencial nenhuma**, e este código nunca lê, copia ou cria uma.
Quem autentica é o `claude login` do cliente oficial, em cada diretório.

---

## O contrato da ponte

Ações do `window.__shviaCode` (shim em `src-tauri/src/code_bridge.rs`):

| ação | entrada | resposta |
|---|---|---|
| `claudeAccounts()` | — | `{contas:[{id,rotulo,disponivel,motivo}], selecionada}` |
| `claudeAccountSelect(id)` | `{accountId}` | `{selecionada}` · `{erro,codigo}` |
| `claudeModels({accountId})` | `{accountId}` | `{modelos:[…]}` · `{erro,codigo}` |
| `spawn({…, accountId})` | `{accountId}` | `{ok:true, accountId, accountLabel}` |

**Nenhum caminho atravessa a ponte** — nem na entrada, nem na resposta, nem na mensagem de
erro. A página manda um `id`; quem traduz é o Rust.

**A capacidade é declarada por presença:** `recursos.conta` no shim. Ausência é a resposta
"não" — é assim que uma casca antiga com uma página nova continua com a régua de antes, em
vez de mostrar um seletor que ela ignoraria. Mesmo desenho do `recursos.imagem`.

**O `spawn` ecoa a conta que valeu.** Não existe evento `ready` neste protocolo, e criar um
mudaria o `embedding.md`, que é contrato dos dois motores — a resposta do `spawn` é o lugar
que já existe para a página saber que o pedido foi obedecido em vez de supor.

### Códigos de erro

| código | quando | o que a página faz |
|---|---|---|
| `conta_desconhecida` | id fora do registro | bloqueia o envio; recarrega a lista |
| `conta_indisponivel` | id conhecido, pasta sumiu | bloqueia o envio; opção desabilitada com motivo |

**Não há braço de fallback.** Cair na conta padrão quando o id não resolve rodaria o turno
numa assinatura que o usuário não escolheu, com a tela mostrando o nome da que ele
escolheu — e sob assinatura isso gasta a cota da conta errada.

---

## O "Conectar meu CLI" segue o perfil também

`cli_config.rs` ([ADR-026](../decisoes.md#adr-026)) grava as três variáveis `ANTHROPIC_*`
no `settings.json` do Claude Code, para apontar o CLI ao gateway do ShvIA. O destino era
`~/.claude/settings.json` **fixo**.

🔴 **Isso passou a estar errado na 1.4.16.** Quem estivesse na conta `Empresa · Blue3` e
clicasse em "Gravar no meu computador" configuraria o diretório da conta **errada**: o `env`
ia para `~/.claude` e o Modo Code seguia lendo `~/.claude-blue3`. Sem erro nenhum, e a
descoberta só viria por alguém perguntar por que a configuração "não pegou".

Desde a 1.4.19 o destino é o diretório do **perfil selecionado** — é configuração do Claude,
então ela segue o perfil de conta do Claude. `padrao` continua em `~/.claude`.

O `conta_dir` vem do registro **nativo**, nunca da página: ela segue mandando só valores e
nem sabe que perfis existem, então o invariante do ADR-026 fica intacto. E o diálogo nativo
de confirmação já mostra o caminho exato antes de gravar — quem está na conta da empresa vê
`~/.claude-blue3/settings.json` na tela e decide.

**Os outros clientes não seguem a conta.** `~/.continue` e `~/.shvia` não são do Claude;
fazer o perfil dele mover o arquivo de outro seria efeito colateral.

Perfil que não resolve **falha alto**, pela mesma razão do `spawn`: gravar em `~/.claude`
como consolo seria escrever numa conta que o usuário não escolheu.

## Onde a conta é aplicada

`contas_claude::resolver` é a **única** tradução id→diretório, e `contas_claude::aplicar`
põe `CLAUDE_CONFIG_DIR` no `Command` do filho. Os dois pontos que sobem um `claude-runner`
passam por ela:

1. `claude_models(conta_dir)` — o `--modelos`, que pergunta o catálogo ao SDK;
2. o ramo `is_claude` do `spawn` — o turno.

Enquanto o `claude_models` não pedia conta nenhuma, os dois eram caminhos independentes até
o mesmo binário, e nada obrigava a concordarem. O catálogo **é** por assinatura: discordar
significa oferecer na tela um modelo que o turno vai recusar.

🔴 **`Command::env`, nunca `std::env::set_var`.** Duas janelas podem estar em duas contas ao
mesmo tempo. Uma variável de processo faria a janela que spawnou por último decidir pela
outra, e a outra continuaria mostrando na tela a conta que ela escolheu.

### Precedência de autenticação

O `claude-runner` remove `ANTHROPIC_API_KEY` **deste processo** antes de qualquer porta do
SDK (1.4.14) — sem isso, o SDK daria precedência à chave e a sessão viraria pay-per-token
em silêncio. A remoção é do filho; o ambiente do pai fica intacto. Nada de proxy ou
configuração de rede corporativa é tocado: só essa variável, que é a única cuja presença
troca **quem paga**.

---

## 🔬 Como se sabe que o `CLAUDE_CONFIG_DIR` é obedecido — e por que pasta ausente é recusa

Medido em 05/09/2026, rodando o `--modelos` (canal de controle, não gasta turno) sob três
diretórios. O sinal observável é a **descrição do modelo `default`** no catálogo:

| `CLAUDE_CONFIG_DIR` | descrição do `default` |
|---|---|
| `~/.claude-blue3` | `Opus 5 with 1M context · Best for everyday, complex tasks` |
| `~/.claude-pessoal` | idem |
| diretório vazio | `Use the default model (currently Opus 5 (1M context)) · $5/$25 per Mtok` |

As duas contas dão o mesmo catálogo — são a mesma faixa de assinatura, então isso não as
distingue. **O que prova que a variável é obedecida é o terceiro caso:** o SDK criou
`.claude.json`, `projects/`, `sessions/` e `backups/` **dentro do diretório vazio** e caiu
na descrição de pay-per-token, que é a mesma que aparece quando há `ANTHROPIC_API_KEY` no
ambiente (ver 1.4.14).

🔴 **É por isso que `resolver` recusa uma pasta que sumiu em vez de repassá-la.** O SDK não
reclama de um diretório inexistente: ele **cria** e segue sem assinatura. Um perfil cujo
diretório foi movido rodaria o turno fora da assinatura, em silêncio, com o nome da conta
certa na tela. `conta_indisponivel` existe para que esse caso pare antes do spawn.

### E o turno de verdade, medido em cada conta

O `--modelos` é canal de controle: ele prova que a variável chega, não que a **autenticação**
muda. Rodado em 05/09/2026, com o mesmo argv e o mesmo ambiente que o `spawn` monta
(`--cwd <projeto> --aprovacao manual` + `CLAUDE_CONFIG_DIR`), pedindo uma palavra:

| `CLAUDE_CONFIG_DIR` | resposta | tokens |
|---|---|---|
| `~/.claude-blue3` | `ok` (modelo `claude-opus-5[1m] · anthropic (assinatura)`) | 6 |
| `~/.claude-pessoal` | `ok` (idem) | 6 |
| diretório sem login | `Not logged in · Please run /login` | **0** |

A terceira linha é o controle negativo, e é ela que fecha o argumento: as duas primeiras
poderiam ser a mesma conta respondendo duas vezes. **É a variável que decide quem
autentica.**

⚠️ **E ela mede exatamente o que `disponivel` NÃO promete.** Um diretório que existe e não
tem login vale `disponivel: true` e mesmo assim não roda turno — o erro vem do cliente
oficial, na hora do turno, porque é ele que sabe. Nada aqui tenta adivinhar isso antes.

---

## 🔬 macOS: the variable that separates the owner's accounts is another one

Measured 08/09/2026 on the owner's Mac (macOS 25.6, `claude` 2.1.265 arm64, Agent SDK
0.3.258). The section above was measured on **Linux**; this one contradicts part of it, and
the contradiction is the point.

**The account picker offers one entry here.** `~/.claude-blue3` and `~/.claude-pessoal` do
not exist on this machine, so `semente()` seeds nothing and
`~/Library/Application Support/cloud.blue3.shvia/contas-claude.json` is
`{"contas": [], "selecionada": "padrao"}`. Nothing is broken — the seed is doing exactly
what it was written to do. It was written against a machine whose layout this one does not
share.

**The two aliases are not aliases and do not set `CLAUDE_CONFIG_DIR`.** They are shell
functions (`~/.zshrc`), and each exports **`CLAUDE_SECURESTORAGE_CONFIG_DIR`** —
`claude-me` at `~/.claude-cred-pessoal`, `claude-b3` at `~/.claude-cred-blue3` — before
`exec claude`. (They also source `~/.config/ai-memory/env-<account>`; that directory does
not exist on this machine, so those lines are guarded no-ops today.)

### What each variable actually keys, read from the client

From the credential-store code in the 2.1.265 binary, the Keychain **service name** is
built like this:

| environment | service name |
|---|---|
| neither variable | `Claude Code-credentials` |
| `CLAUDE_CONFIG_DIR=D` only | `Claude Code-credentials-<sha256(D)[0:8]>` |
| `CLAUDE_SECURESTORAGE_CONFIG_DIR=D` | `Claude Code-credentials-<sha256(D)[0:8]>` |
| both set | the **securestorage** one wins; `CLAUDE_CONFIG_DIR` is not consulted for the key |

🔴 **The two variables feed the same key.** The difference is everything *else*
`CLAUDE_CONFIG_DIR` does: it also moves the configuration home — settings, history,
`projects/`, `sessions/`. `CLAUDE_SECURESTORAGE_CONFIG_DIR` moves the credential key and
**nothing else**, which is what "shared configuration, separate logins" means.

### Three logins exist on this Mac

Checked by **presence only** — the item's attributes, never its content, never `-w`, no
authorization prompt. This is to a credential what `is_dir()` is to a file.

| what would key it | service name | on this Mac |
|---|---|---|
| neither variable (what the Desktop does today on `padrao`) | `Claude Code-credentials` | **present** |
| `~/.claude-cred-blue3` (`claude-b3`) | `Claude Code-credentials-851c8232` | **present** |
| `~/.claude-cred-pessoal` (`claude-me`) | `Claude Code-credentials-b3996ef2` | **present** |
| `~/.claude-blue3` as `CLAUDE_CONFIG_DIR` | `Claude Code-credentials-a42ff1ff` | absent |
| `~/.claude-pessoal` as `CLAUDE_CONFIG_DIR` | `Claude Code-credentials-e6564ce4` | absent |

The two suffixed items could only have been written by a `claude login` run with
`CLAUDE_SECURESTORAGE_CONFIG_DIR` set to those exact paths. **That is what proves the
variable is honoured here** — not a catalogue, not a turn.

**On macOS the securestorage directory is a key string, not a store.** Both
`~/.claude-cred-*` are empty, were empty before these measurements and are empty after: the
credential is in the Keychain, and the path is only hashed. `disponivel` — "the directory
exists" — therefore says even less on macOS than the ADR already admits it says. It reports
that the owner ran `mkdir`.

### The failure of registering a `-cred-` directory as `dir`, corrected

The obvious shortcut is to put `~/.claude-cred-blue3` in `contas-claude.json` as a `dir`,
since the field already accepts any absolute path inside `$HOME`. `aplicar()` would export
it as `CLAUDE_CONFIG_DIR`. What happens next is **not** the same on the two operating
systems, and neither outcome is acceptable:

- **On Linux** (measured 05/09, table above): the directory has no login, so the turn dies
  with `Not logged in · Please run /login`, or worse runs off-subscription.
- **On macOS** (derived here from the keying above): the hash is the *same* whichever
  variable carries the path, and `Claude Code-credentials-851c8232` exists — so the client
  finds the **right credential** and pairs it with a **blank configuration home**, which it
  populates on the spot. No error. The account is right; the settings, history and project
  state silently fork.

One registry entry, two different wrong answers depending on the operating system. That is
the argument for recording **which variable** a profile means instead of inferring it from
a path.

### `--modelos` does not discriminate accounts on macOS — do not re-propose it

The Linux section uses the `default` model's description as the observable signal. Re-run
here under a clean environment (`env -i` with `HOME`/`PATH`/`USER`), the catalogue is
**identical** with no variable and with `CLAUDE_SECURESTORAGE_CONFIG_DIR` at either
directory. It is not a discriminator on this platform and this client version; any macOS
check built on it would be reading noise.

⚠️ **And a warning for whoever measures next:** the first run of this comparison was made
from inside a Claude Code session and *did* show a difference — because that session's
environment carries `ANTHROPIC_BASE_URL` and a set of `CLAUDE_CODE_*` host-auth variables.
The difference was the surrounding session, not the variable under test. Measure from a
scrubbed environment or do not measure at all.

**Still not measured on macOS:** a real turn under each account. It is the only thing that
closes the argument the way the Linux table closes it, and it spends quota on two
subscriptions — the owner decides whether to spend it, not this document.

## O que `disponivel` não diz

`disponivel` é **o diretório existir**. Não é:

- que o login lá dentro é válido;
- que ele não expirou;
- que a conta pertence à organização escrita no rótulo.

Os rótulos são palavra do usuário — `contas-claude.json` é editável à mão. Dizer
"conectado" a partir da existência de uma pasta seria afirmar na tela algo que este código
não verificou, e a hora de descobrir que era mentira seria no meio de um turno.

---

## Histórico e transcrito

O histórico do Modo Code (`shvia.codeHist:<pasta>`, no `localStorage`) é **registro de
leitura**: ele repinta texto de usuário e de assistente numa timeline vazia e sempre carimba
*"o agente não lembra do que está acima"*. Nada dele volta para o SDK — o contexto do modelo
é só o `resume` do `sessionId`, que morre junto com o processo.

Conferido em 05/09/2026 (`histPintar`/`histLer` em `SHVIA-WEB/public/js/code-mode.js`): por
isso um transcrito restaurado **não pode** virar contexto de outra conta, e o histórico
**não** foi rechaveado por conta. Rechavear apagaria o histórico de todo mundo sem fechar
risco nenhum.

---

## Não coberto

- **Windows.** O `resolve_bin` e o lugar onde o SDK guarda credencial mudam por SO. O
  `USERPROFILE` está no código e não foi rodado.
- **macOS: o turno.** Desde 08/09/2026 o *chaveamento* da credencial está medido (seção
  acima) e o `CLAUDE_SECURESTORAGE_CONFIG_DIR` está identificado como a variável que separa
  as contas do dono. O que continua sem medida é um **turno de verdade** sob cada conta — o
  `--modelos` não discrimina nesta plataforma, e o único método que fecharia o argumento
  gasta cota de duas assinaturas.
- **macOS: nenhum perfil é oferecido hoje.** O seletor mostra só `Padrão do sistema`, e a
  proposta de como consertar isso — qual variável, como registrar, como reportar login —
  está em [`.continue/contas-claude-macos.md`](../../.continue/contas-claude-macos.md),
  ainda **não decidida**.
- **Gerenciar organização.** Nada aqui cria conta, faz login ou sabe de membership.

