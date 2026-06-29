# SHVIA-DESKTOP — Estado e Continuidade

> Notas de continuidade (ler primeiro). Rebrand iniciado em 24/06/2026, executado em 29/06/2026.

## O que é

**SHVIA-DESKTOP** é um rebrand de **`aaddrick/claude-desktop-debian`** que empacota o
**Claude Desktop** (app da Anthropic) para Linux — gera `.deb`/`.rpm`/AppImage e flake Nix.
Mantido sob a conta **`samirhvbr`** como projeto próprio (repo standalone, privado).

- O **pacote** instalado chama-se `shvia-desktop` (binário `/usr/bin/shvia-desktop`,
  libs em `/usr/lib/shvia-desktop`, ícone/`.desktop` `shvia-desktop`, component id
  `io.github.samirhvbr.shvia-desktop`, cache em `~/.cache/shvia-desktop`).
- O **app em si continua sendo o Claude Desktop**: `WM_CLASS`/`productName` = `Claude`
  (o build falha se divergir — guard em `scripts/patches/app-asar.sh`), rótulo de menu
  `Name=Claude`, esquema `claude://`, config em `~/.config/Claude/`. Atribuição ao
  aaddrick e à Anthropic preservada (LICENSEs, créditos no README, descrições
  "unofficial / not affiliated with Anthropic").

## Git / remotes

| Remote | URL | Papel |
|--------|-----|-------|
| `origin` | `git@github.com:samirhvbr/SHVIA-DESKTOP.git` | **principal** (privado) |
| `fork` | `git@github.com:samirhvbr/claude-desktop-debian.git` | backup / base p/ PRs ao upstream |
| `upstream` | `git@github.com:aaddrick/claude-desktop-debian.git` | original (push desabilitado) |

- Branch principal: **`main`**.
- Sincronizar com o upstream: `git fetch upstream && git merge upstream/main`
  (vai conflitar nos arquivos rebrandeados — resolver mantendo o branding SHVIA-DESKTOP).

## Convenções

- **Commits:** `versão - comentário` (ex.: `0.2.0 - rebrand SHVIA-DESKTOP`). Versão em
  `version.md` (hoje **0.2.0**), incrementada a cada mudança relevante. Mensagens em pt-BR.
- **`.claude/`:** perfis de modelo Opus/Fable (ver `.claude/README.md`); coexiste com o
  `.claude/` herdado do upstream (agents/hooks/skills).

## O que ainda falta (infra de distribuição — categoria D)

Infra herdada do upstream que **não roda** no fork sem provisionamento próprio:

- `worker/` (Cloudflare Worker, domínio `pkg.claude-desktop-debian.dev`).
- Workflows de repo apt/dnf, `gh-pages`, deploy-worker, heartbeat e triagem por IA
  (precisam de secrets do upstream: Cloudflare, GPG, Anthropic API).
- **Plano:** desabilitar/remover esses workflows no repo novo (ou configurar os próprios).
  Build local (`./build.sh`) e os testes de artefato já estão rebrandeados.

## Próximos passos

1. Desabilitar os workflows de distribuição (D) no SHVIA-DESKTOP.
2. (Opcional) `./build.sh --build deb` no Trixie e validar o `.deb` `shvia-desktop`.
3. (Opcional) Primeiro tag/release `v0.2.0` no repo novo.
