> **Status on 08/09/2026, when this note moved here from the route repository:** the
> delivery below is in `master` (1.4.13–1.4.20); `SHVIA-WEB` PR #71 was **merged on 07/09**
> (2.110.186) and the worktree `SHVIA-WEB-conta-claude` is gone. What still needs a
> person is §"Precisa de você" items 1 and 2: the six-step manual validation in the app
> and the macOS/Windows run. Item 3 is done.

# Onde paramos — perfis de conta do Claude Code (05→06/09/2026)

> Diretório ignorado pelo git (`.continue/code/`), como a proposta original.
> A doc definitiva mora em `SHVIA-DESKTOP/docs/code/CONTAS-CLAUDE.md` + ADR-033.

A proposta `claude-account-profiles.md` que estava aqui **foi implementada**. Este
arquivo é só o estado da entrega e o que sobrou para amanhã.

---

## Precisa de você

**1. Validação manual no app (item 2).** Bloqueada por ambiente: a sessão do agente não
tem `DISPLAY`, nem Wayland, nem Xvfb. Exige a sua sessão de desktop.

```bash
cd ~/x/SHVIA/SHVIA-DESKTOP && npm run tauri dev
```

Os seis passos, e nenhum deles gasta turno:

1. Motor **Claude Code** → o pill **CONTA** aparece **no lugar do INFRA**; no Chat, some.
2. Trocar para `Empresa · Blue3` → fronteira na timeline (`conta alterada para …`),
   MODELO recarrega, e o `accountId` sai no `spawn` (visível no log do app).
3. **Duas janelas em contas diferentes ao mesmo tempo** — é a prova real do
   `Command::env` contra `std::env::set_var`. Nenhuma pode sobrescrever a outra.
4. `mv ~/.claude-pessoal ~/.claude-pessoal.off` → a opção vem **desabilitada com motivo**
   e o `spawn` **recusa** em vez de cair na conta padrão. (Devolva o nome depois.)
5. Trocar de conta com turno em voo, e de novo com cartão de aprovação aberto → o
   seletor fica **travado, com o motivo no `title`**.
6. Casca antiga (1.4.12) + web nova → **nenhum pill CONTA**, INFRA como antes.

**2. macOS e Windows (item 4).** Não dá desta máquina. Tentei o caminho mais próximo —
`cargo check --target x86_64-pc-windows-msvc` — e ele para em
`failed to find tool "lib.exe"`: falta a toolchain MSVC. E mesmo passando não provaria o
que importa, que é **onde o SDK guarda credencial em cada SO** e o `resolve_bin`.

Se você tiver acesso a um Mac ou a um Windows, peça o roteiro e eu monto. Senão fica
declarado como não validado — que é o que o ADR-033 e o `CONTAS-CLAUDE.md` já dizem, com
todas as letras.

**3. Limpeza, quando quiser.** A worktree `SHVIA-WEB-conta-claude` tem um `.env` em `600`
dentro. Sai com `git worktree remove ../SHVIA-WEB-conta-claude` — **nunca `rm -rf`**, que
apaga o segredo e deixa a bookkeeping. E estes dois arquivos da proposta
(`claude-account-profiles.md` / `.svg`) podem sair: o conteúdo virou
`docs/code/CONTAS-CLAUDE.md`.

---

## Entregue

### SHVIA-DESKTOP — `master`, CI verde, Release `Latest`

| versão | assunto |
|---|---|
| 1.4.13 | 🔴 devolve o conjunto de ferramentas de edição que o hook de permissão tinha perdido |
| 1.4.14 | auth de assinatura normalizada antes de **toda** porta do SDK |
| 1.4.15 | pin por SHA do `checkout` no workflow de release |
| 1.4.16 | **descoberta e spawn rodam sob a conta selecionada** (ADR-033) |
| 1.4.17 | a mensagem de falha nomeia o motor certo |
| 1.4.18 | 🔴 stand-in do sidecar: o `cargo test` do CI **nunca** tinha rodado |
| 1.4.19 | "Conectar meu CLI" grava na conta selecionada, não num caminho fixo |
| 1.4.20 | registra o turno medido por conta |

### SHVIA-WEB

- **PR #72 — `2.110.185`: MERGEADO.** A corrida de catálogo.
- **PR #71 — `2.110.186`: aberto, rebaseado e carimbado.** O seletor CONTA.
  Falta só o CI fechar na base nova e mergear. Se estiver verde quando você acordar,
  pode mergear direto — a suíte completa rodou verde aqui (3175/3174/1 pulado).

---

## Três defeitos vivos que apareceram no caminho (nenhum era do pedido)

**1. O hook de permissão do Claude Code estava fora do ar há dois dias.**
`claude-runner.mjs` passava `edicao: EDIT_TOOLS`, e `EDIT_TOOLS` não existia — a
definição saiu na 1.4.7 e a referência ficou. O literal é montado **antes** de `decidir()`,
então o `preToolUse` lançava `ReferenceError` em **toda chamada de ferramenta, em qualquer
nível de aprovação**. É a fronteira do ADR-032 — o cartão de aprovação — desligada.

A prova não pegou porque **declarava a própria cópia do conjunto**. Agora os dois vêm de
`politica.mjs`, e o gatilho da metade que teste de conteúdo não cobre é
`npm run prova:runner-version` (import nomeado inexistente em ESM falha ao **ligar** o
módulo, e `--version` responde antes do import do SDK).

**2. Descoberta autenticava diferente do turno.** O bloco que remove `ANTHROPIC_API_KEY`
estava **abaixo** do braço `--modelos`, que sai com `process.exit()`. Medido com chave
falsa: o catálogo voltava descrevendo `$5/$25 per Mtok` em vez do modelo de assinatura. O
seletor MODELO mostrava rótulo de pay-per-token para uma sessão que rodava na assinatura.

**3. O CI do SHVIA-DESKTOP nunca tinha ficado verde.** Zero sucessos em 13 execuções.
Três guardas empilhadas: portadores de versão (desde a 1.4.9) escondiam uma action sem pin
por SHA, que escondia o `cargo test` que **nunca compilou** — `binaries/anna-*` é
gitignored e o `externalBin` do Tauri exige o arquivo. Reproduzido com clone limpo.

> **O padrão dos três é o mesmo, e vale mais que os três:** *execução vermelha reporta o
> primeiro passo que falha e não diz nada sobre o resto.* Cada correção destapou a
> seguinte. Não há como saber, olhando um X vermelho, se os passos depois dele estão bons
> ou só nunca foram olhados.

---

## Medições que mudaram o desenho (não foram argumento)

**Pasta de configuração vazia não faz o SDK reclamar.** Ele **cria** `.claude.json`,
`projects/`, `sessions/` lá dentro e segue **sem assinatura**. Por isso pasta ausente é
**recusa** (`conta_indisponivel`), não repasse: um perfil cujo diretório foi movido rodaria
o turno fora da assinatura, em silêncio, com o nome da conta certa na tela.

**O turno real, por conta** (05/09, com o argv e o ambiente que o `spawn` monta):

| `CLAUDE_CONFIG_DIR` | resposta | tokens |
|---|---|---|
| `~/.claude-blue3` | `ok` · `claude-opus-5[1m] · anthropic (assinatura)` | 6 |
| `~/.claude-pessoal` | `ok` · idem | 6 |
| diretório **sem login** | `Not logged in · Please run /login` | **0** |

A terceira linha é o controle negativo e é ela que fecha o argumento — as duas primeiras
sozinhas podiam ser a mesma conta respondendo duas vezes. **É a variável que decide quem
autentica.** Custou 12 tokens de cota, com autorização.

**`hidden` sozinho não esconde os pills.** `.chat-topbar__select` declara
`display:inline-flex`, e regra de autor vence o `[hidden]` do agente do usuário. O
`code-mode.css` usa `:not([hidden])` e um seletor mais específico. ⚠️ Isso foi conferido
**lendo a cascata**, não medido: o harness é linkedom e `getComputedStyle(...).display`
volta `null` ali. O que as provas do WEB medem é o atributo, não o pixel.

---

## Duas coisas que o próprio repo ensinou no caminho

**O `versao-no-merge.sh` recusou minha branch** — *"há 2 marcadores — mais de um significa
branch com duas entregas"*. Conferi como o repo faz (PR #69: 2 commits → 1 entrada): a
convenção é **um PR = uma entrega**. Não contornei o guard; separei em #72 e #71.

**`vendor/` como symlink numa worktree faz o autoloader resolver `Tests\` para o checkout
principal.** Os testes rodaram o harness *do outro diretório* e passaram sem medir nada —
verde por ausência, a mesma família do `markTestSkipped` do §13. Troquei por cópia real.
Vale um item de fila: o `deploy/dev/README.md` manda copiar o `.env` para a worktree, mas
não diz nada sobre `vendor/`.

---

## Fora de escopo, declarado

- **Gerenciar organização.** Isto escolhe entre perfis locais que já existem; não cria
  conta, não faz login, não sabe de membership.
- **Rechavear o histórico por conta.** Conferido em `histPintar`/`histLer`: o histórico é
  registro de leitura e **nunca volta para o SDK** — o contexto do modelo é só o `resume`
  do `sessionId`, que morre no `resetSession`. Rechavear apagaria o histórico de todo mundo
  sem fechar risco nenhum.
