# claude-runner — motor "Claude Code (assinatura)" do Modo Code

Motor **paralelo** do Modo Code que orquestra o **cliente oficial** (Claude Agent
SDK) usando a **assinatura Pro/Max do usuário** e fala o **mesmo NDJSON do `anna`**
(`SHVIA-CODE/docs/embedding.md`). Por falar o mesmo protocolo, é **drop-in** atrás
do `code_bridge.rs` — o bridge e a UI de cards não mudam. O `code_bridge.rs`
escolhe o motor pelo campo `engine` do `spawn` (`engine:'claude'` → este runner;
ausente/`'gateway'` → `anna`).

## ⚠️ Fora do gateway

Este motor manda a inferência **direto do `claude` para a Anthropic** com a
assinatura do usuário — **sem passar pelo gateway do SHVIA**. Logo, **não há**
auditoria (`inference_requests`), quota, mascaramento LGPD nem medidor "Uso de
API" neste modo. É um **toggle paralelo** ("Motor: SHVIA gateway | Claude Code
assinatura"), não um substituto do `anna`.

## Instalação

```bash
./install.sh            # → ~/.local/bin/claude-runner (padrão anna)
```

Pré-requisitos: **Node 18+** e o **Claude Code oficial autenticado**
(`claude login` ou `claude setup-token`). A assinatura é usada; **sem API key**.
O runner remove `ANTHROPIC_API_KEY` do próprio processo para garantir o fallback
à assinatura.

## Protocolo

Igual ao `anna` (`embedding.md`):

- **stdin**: `{"type":"user","text":…}` · `{"type":"exit"}` · `{"id":…,"decision":"approve"|"always"|"reject"}`
- **stdout**: `model` · `text{delta}` · `tool_call` · `tool_result` · `gate_request` · `usage` · `turn_done` · `error`/`warn`

## Permissão (a regra "nada roda/escreve sem o dev ver")

A política é um **PreToolUse hook + `settingSources: []`** (o hook é a autoridade
única e bypassa as allow-rules; o settings-vazio impede o `~/.claude/settings.json`
pessoal do usuário de auto-aprovar). Leitura (`Read`/`Glob`/…) = auto; o resto vira
`gate_request` (bloqueia) → decisão do card → `allow`/`deny`. O `gate_request.id`
é o `tool_use_id` real (casa com o `tool_result`).
