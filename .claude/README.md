# Perfil de modelo Claude Code — ShvIA Desktop

`.claude/` deste projeto segue o padrão dos repos Blue3/samirhvbr: perfil de
modelo + postura de permissões. Stack-alvo: **Tauri 2 (Rust) + casca web
(npm/Vite) + sidecar Python (entra na F2)** — o servidor é remoto, então
**nenhum banco ou segredo roda aqui**.

## Arquivos

| Arquivo | Papel |
|---------|-------|
| `settings.json` | Perfil **ativo** (versionado). Hoje = **Opus-only**, `defaultMode: plan`, só a **deny-list** de segurança. |

> A **allow-list** (atalhos que evitam prompts repetidos) **não** vem no
> `settings.json` de propósito: conceder permissão ao agente é uma ação **sua**.
> Aplique o bloco abaixo manualmente quando quiser reduzir os prompts.

## Allow-list recomendada (cole em `permissions.allow`)

```jsonc
"allow": [
  "Read", "Edit", "Write",
  "Bash(git status:*)", "Bash(git diff:*)", "Bash(git log:*)",
  "Bash(git show:*)", "Bash(git branch:*)",
  "Bash(git add:*)", "Bash(git commit:*)", "Bash(git push:*)",
  "Bash(node -c:*)", "Bash(node --check:*)",
  "Bash(npm run dev:*)", "Bash(npm run build:*)", "Bash(npm run tauri:*)",
  "Bash(npm install:*)", "Bash(npx tauri info:*)",
  "Bash(cargo check:*)", "Bash(cargo build:*)", "Bash(cargo test:*)",
  "Bash(cargo fmt:*)", "Bash(cargo clippy:*)",
  "Bash(python -m py_compile:*)", "Bash(python3 -m py_compile:*)",
  "Bash(pytest:*)", "Bash(bats:*)", "Bash(shellcheck:*)"
]
```

## Regras que valem lembrar

- **Effort `max` vai por env** (`CLAUDE_CODE_EFFORT_LEVEL=max`). O campo
  `effortLevel` do JSON só aceita `low/medium/high/xhigh` — `max` ali é ignorado.
- **1M é nativo** no Opus 4.8 (API Anthropic), sem flag. Não setar
  `CLAUDE_CODE_DISABLE_1M_CONTEXT`. No plano Max é incluso — usar longe do limite.
- **`defaultMode: plan`** — o agente planeja antes de agir. Mantém o hábito de
  revisar mudanças estruturais antes de tocar em código.

## Deny-list (já no `settings.json`)

Bloqueia leitura de `.env`/chaves (`*.pem`/`*.key`/`*.p8`/`*.p12`/`*.pfx`),
`rm -rf`, `git push --force/-f`, `git reset --hard`, `git clean -fd` e
`curl|sh`/`wget|sh`. **Não afrouxar** sem motivo documentado.
