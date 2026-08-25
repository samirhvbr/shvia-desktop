# F0 — Mapa real (Modo Code) — aguardando aprovação

> **Data:** 2026-07-09 · **Fase:** F0 (mapeamento, SEM código) · **Status:** aguardando ratificação do Samir
> **Spec:** [MODO-CODE-20260709.md](MODO-CODE-20260709.md) · **Mockup:** [modo-code-mockup.html](modo-code-mockup.html)
> **Repos lidos:** app `~/x/SHVIA/SHVIA-DESKTOP` (0.5.8, master) · `anna` `~/x/SHVIA/SHVIA-CODE` (0.5.3, master)

## 0. Resumo (as 3 conclusões que importam)

1. **O `anna` (motor) está pronto** para este caso — é literalmente o que o arco
   0.5.1–0.5.3 construiu (`--json`, perfil de ferramentas **local**, loop com
   read/edit/grep/git/bash + aprovação bloqueante). Faltam **2 knobs de protocolo**
   (§5.1) — sendo um deles exatamente o **esforço**.
2. **O app é casca-fina pura, e ASSIM CONTINUA** (correção do Samir, 09/07). A
   **UI do Modo Code mora no SHVIA-WEB** (Blade), carregada no webview — igual ao
   Chat; a troca `Chat|Code` é do próprio web. O **SHVIA-DESKTOP só provê a ponte
   nativa**: spawn do `anna`, file picker, FS, git status — exposta à página via
   **bridge injetado** (mesmo mecanismo já usado p/ TTS e clipboard). **Nada de SPA
   local, nada de duas webviews.** Repartição de trabalho em §5.2.
3. **As 2 lacunas do `anna`** (§4.1) — sendo uma o **esforço** — são o F1.

---

## 1. Lado `anna` (motor) — PRONTO

### 1.1 Eventos NDJSON reais (`SHVIA-CODE/src/json.rs`)

Uma linha JSON por evento no **stdout**:

| `type` | Campos | Quando |
|--------|--------|--------|
| `model` | `model`, `server` | início do turno (modelo/infra escolhidos) |
| `text` | `delta` | pedaço da resposta (streaming) |
| `tool_call` | `id`, `name`, `arguments` | a Anna vai usar uma ferramenta |
| `tool_result` | `id`, `name`, `bytes`, `content` | resultado (já executado) |
| `gate_request` | `scope`, `policy`, `preview` | **pede aprovação — BLOQUEIA** |
| `usage` | `tokens`, `cost`, `estimated` | telemetria do turno |
| `turn_done` | — | turno terminou |
| `error` / `warn` | `message` | erro fatal / aviso |

`preview` ∈ `{kind:"diff", path, diff}` · `{kind:"command", command, why}` ·
`{kind:"commit", stat, message}`. `policy` ∈ `confirm` | `always`.

**stdin** (app → `anna`): `{"type":"user","text":…}` inicia um turno;
`{"decision":"approve"|"always"|"reject"}` responde um `gate_request`;
`{"type":"exit"}` encerra.

### 1.2 Flags CLI reais (`SHVIA-CODE/src/main.rs`)

`--json` (modo motor) · `--url <URL>` (gateway) · `--model <perfil>` (perfil
`modelo@servidor`) · `--tools local|host` (**Code usa `local`**, default) · `-p`
(one-shot). **cwd do spawn = pasta do projeto** (detecção de git/ANNA.md usa o
diretório de trabalho — **P3 ✓, sem precisar de `--dir`**). Chave via
**`SHVIA_API_KEY`** no ambiente (**P4 ✓**).

### 1.3 Aprovação e confinamento

- **P2 (bloqueio):** o `gate_request` bloqueia lendo a próxima linha de decisão
  do stdin — **✓ o mecanismo existe**, MAS a correlação é por **ordem**, não por
  `id` (ver lacuna §5.1).
- **Confinamento (§8 do spec):** `guard::resolve` canonicaliza e exige prefixo da
  raiz do projeto; denylist de segredos (`.env*`, chaves) vale na leitura e na
  escrita — **✓** quando spawnado com `cwd` = pasta vinculada.

---

## 2. Lado app (casca) — A CONSTRUIR

Referências reais (path:linha):

| Área | Hoje | Arquivo |
|------|------|---------|
| Entrada/boot | splash → `location.replace(SHVIA_URL)` | `src/main.ts:23,103-124` |
| UI local | **nenhuma** (só splash/offline) | `index.html`, `src/styles.css` |
| Menu | nativo Arquivo/Ajuda (sem "Ver") | `src-tauri/src/lib.rs:499-579` |
| `#[tauri::command]` | **nenhum** | — |
| Spawn de processo | só TTS, **mão única** (sem stdio) | `src-tauri/src/lib.rs:418-459` |
| Plugins | `opener`, `window-state`. **Faltam** `shell`, `dialog`, `fs`, `store` | `src-tauri/Cargo.toml` |
| Capabilities | só `core:default`, `opener:default` | `src-tauri/capabilities/default.json` |
| Config local | só geometria de janela | `tauri-plugin-window-state` |
| Tokens/fontes | Azure (#34B3EC), sem terracota/Montserrat/Tabler | `src/styles.css:10-20` |

**Implicação (revisada):** a coluna 3 (timeline), a coluna 2 (painel da pasta) e
o segmento `Chat|Code` são **UI do SHVIA-WEB** (Blade + JS), **não** do desktop.
O desktop hoje não tem UI local — e não precisa ter: ele só ganha a **ponte
nativa** (§4.2). O que o app injeta é um flag `window.__shviaDesktop` (p/ o web
mostrar o Modo Code só no desktop) + as funções `window.__shviaCode.*`.

---

## 3. Mapeamento evento `anna` → componente (mockup)

| Evento / origem | Componente da UI |
|-----------------|------------------|
| stdin `{type:user}` | balão do usuário (direita) |
| `model` | inicia turno; alimenta o rodapé (infra/modelo) |
| `text {delta}` | prosa streamando |
| `tool_call` read-only (`read_file`/`grep`/`list_dir`/`glob`/`git_status`/`git_diff`) | **linha discreta** colapsável (ícone + `nome args`) |
| `gate_request` `preview.kind=diff` | **card de diff** (âmbar → Rejeitar/**Aprovar**) |
| `gate_request` `preview.kind=command` | **card de comando** (`why` sempre visível → Rejeitar/**Rodar**) |
| `gate_request` `preview.kind=commit` | card de commit (stat + mensagem) |
| `tool_result` `name=edit_file/write_file` | badge do card (aprovado/gravado) |
| `tool_result` `name=bash` | corpo do card (saída + `[exit N]`, colapsado) |
| `usage` | rodapé de métricas (tokens; **tempo o app cronometra**) |
| `turn_done` | libera o composer |
| `error`/`warn` | card de erro |

**Sutileza de render (importante):** para `edit_file` o `anna` emite
`tool_call` → `gate_request` (com o diff) → `tool_result`. **O card se constrói
do `gate_request`** (é ele que carrega o diff), não do `tool_call`. Para tools
read-only não há `gate_request` — só `tool_call` → `tool_result` (linha discreta).

---

## 4. Lacunas + assinaturas propostas

### 4.1 `anna` (F1 — repo SHVIA-CODE, testável e isolado)

- **L1 · `id` no `gate_request`** (P2, requisito duro §5 do spec). Hoje a
  correlação card↔decisão↔resultado é por ordem. Proposto: incluir `id` (= o
  `tool_call.id`) no `gate_request` e aceitar `{"id":…,"decision":…}` no stdin;
  assim `tool_call.id == gate_request.id == tool_result.id` fecham o card sem
  ambiguidade. *Não quebra o CLI (o terminal ignora o id).*
- **L2 · esforço** (a sua restrição). O `anna` manda `{messages, tools,
  profile_name}` — **sem `effort`**. Proposto: flag `--effort <low|medium|high|…>`
  + campo `effort` no POST `/api/v1/code/chat` (o `CodeChatController` já aceita).
  Assim o seletor de esforço do header **chega** no motor. *Sem mexer na dinâmica
  do select — só passamos o valor adiante.*
- **L3 · (verificação, não código)** confirmar em bancada que o confinamento
  recusa `../` e symlink-escape a partir da pasta vinculada (a leitura do `guard`
  indica que sim). Se falhar, vira item da F1.

### 4.2 App (F2+ — encanamento e SPA)

Plugins a adicionar: `tauri-plugin-shell` (spawn com stdio bidirecional) ·
`tauri-plugin-dialog` (file picker) · `tauri-plugin-fs` (árvore) ·
`tauri-plugin-store` (vínculo projeto→pasta). Capabilities correspondentes.

Comandos Tauri (assinaturas propostas):

```rust
spawn_anna(project_dir, model, server, effort) -> Result<SidecarId>   // stdout→evento
send_to_anna(id, line_json) -> Result<()>                             // user / decision
kill_anna(id) -> Result<()>
pick_folder() -> Result<Option<String>>
list_tree(path, max_depth) -> Result<Vec<FsEntry>>
git_status_porcelain(path) -> Result<GitStatus>                       // read-only, pelo app
set_project_folder(project_id, path) / get_project_folder(project_id)
```

Eventos NDJSON do stdout → emitidos pro front via Tauri `emit` (canal por sessão).

### 4.3 Repartição por repositório (revisada 09/07)

Como o desktop é só a casca, o trabalho de app se divide:

- **SHVIA-WEB (UI):** a view do Modo Code (3 colunas, timeline, cards de diff/
  comando, painel da pasta, segmento `Chat|Code`, seletor). Renderiza igual ao
  Chat; aparece só quando `window.__shviaDesktop` existe.
- **SHVIA-DESKTOP (ponte):** os plugins + comandos Tauri de §4.2, expostos à
  página via bridge `window.__shviaCode.*` (spawn/send/kill do `anna`, `onEvent`,
  `pickFolder`, `pickFiles`, `listTree`, `gitStatus`, `gitDiff`, `get/setBinding`) + o flag
  `__shviaDesktop`.
  - **`gitDiff(path, file)`** (1.2.0, item F6.B1 do SHVIA-WEB) — o diff de **um** arquivo,
    para a aba "Alterações" abrir ao clique. Três decisões que valem saber:
    - ⚠️ **Não pede ao `anna`**, que tem a ferramenta `git_diff`. Seria uma **inferência paga
      para preencher um painel** — e o painel se atualiza sozinho ao voltar o foco da janela,
      então cada alt-tab viraria uma chamada de modelo. Painel é leitura de estado; o
      precedente certo é o `gitStatus` ao lado, não o agente.
    - **`vazio ≠ sem mudança`:** `git diff` compara a árvore contra o ÍNDICE, e um arquivo já
      preparado (`git add`) devolve vazio. Cai no `--staged` e devolve `staged: true`, para a
      página não dizer "sem alterações" a quem acabou de ver o arquivo listado como alterado.
    - **Teto de 256 KB**, cortado em fronteira de **caractere**: o painel pinta o diff linha a
      linha no DOM, e `texto[..N]` em UTF-8 entra em pânico no meio de um multibyte — que num
      diff em português é o caso comum, não o exótico. A resposta traz `truncated`.
- **SHVIA-CODE (`anna`):** as 2 lacunas de §4.1 (F1).

A coexistência Chat↔Code **não** é problema do desktop: é um webview só e a troca
é do web (Blade). Sem duas webviews.

---

## 5. Ratificação pendente (spec §9 + a decisão nova)

- **D1** entrada = segmento `Chat|Code` + menu Ver (recomendado) — **ok?**
- **D2** motor = `anna --json` sidecar (recomendado) — **ok?**
- **D3** vínculo projeto→pasta = config local (`store`) — **ok?**
- **D4** aprovação = manual + toggle "auto-aprovar edições na sessão"; **bash
  sempre manual** — **ok?**
- **D5** sessões code = locais (formato do `anna`) na v1 — **ok?**
- **D6** gating de modelo = mapa local de capacidades (v1) — **ok?**
- **D7** escopo = 1 sessão/projeto, sem multi-abas, saída inline — **ok?**

> **Ratificado pelo Samir (09/07):** D1–D7 conforme recomendado. Arquitetura:
> **UI no SHVIA-WEB, desktop = só ponte nativa** (sem SPA local, sem duas
> webviews). F1 (knobs do `anna`) autorizado a começar.

## 6. Próximo passo proposto

**F1 = os 2 knobs do `anna`** (L1 `id` no gate_request + L2 `--effort`/campo
`effort`), no repo SHVIA-CODE — isolado, com testes e bump de versão. É o único
trabalho que **não depende** das decisões de UI acima, e resolve já a sua
restrição do esforço. As fases F2–F6 (SPA local + encanamento Tauri) começam após
a ratificação da §5.

**Parar aqui e aguardar aprovação do mapa (regra inviolável do spec §13).**
