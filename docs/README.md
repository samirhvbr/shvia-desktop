# Documentação — ShvIA Desktop

Índice da documentação técnica **estável** do projeto. Trabalho em andamento
(WIP) vive em [`../.continue/`](../.continue/) e migra para cá quando amadurece.

## Páginas

| Documento | Conteúdo |
|-----------|----------|
| [funcionalidades.md](funcionalidades.md) | **O que o app já faz** — registro estável das funcionalidades implementadas, por versão. |
| [build.md](build.md) | **Build & empacotamento** — CI dos 3 SOs + scripts locais (`build-local.*`). |
| [decisoes.md](decisoes.md) | **ADRs** — decisões de arquitetura registradas (formato Architecture Decision Record). |
| [arquitetura.md](arquitetura.md) | Arquitetura técnica detalhada: camadas, fluxo de auth, CSP/IPC, build/empacotamento. |
| [roteiro-fundacao.md](roteiro-fundacao.md) | Passo-a-passo da fundação (F0/F1) — comandos concretos para iniciar. |
| [code/MOTOR-CLAUDE-DIAGNOSTICO.md](code/MOTOR-CLAUDE-DIAGNOSTICO.md) | **O motor Claude Code não responde** — runbook por sintoma: o comando que separa "é o app" de "é o login da máquina", em qual cofre cada forma de logar grava, e por que o seletor de contas pode mostrar uma opção só. |
| [code/MOTOR-CODEX-20260909.md](code/MOTOR-CODEX-20260909.md) | **Medição** do Codex como terceiro motor do Modo Code (09/09/2026), com proveniência 🔬 ao vivo / 📋 schema: por que o `exec` foi recusado, as três políticas em que o gate NÃO disparou, a medição M que provou a fronteira, e os cinco defeitos que só o processo real revelou. Fatia 1 (o runner) existe; a ponte e a UI não. |
| [code/CONTAS-CLAUDE.md](code/CONTAS-CLAUDE.md) | Perfis de **conta do Claude Code** no Modo Code (ADR-033): o registro local, o contrato da ponte e onde o `CLAUDE_CONFIG_DIR` é aplicado. |
| [code/CONTAS-CLAUDE-proposta-20260905.md](code/CONTAS-CLAUDE-proposta-20260905.md) | **Archived:** the proposal that became CONTAS-CLAUDE.md (05/09/2026), with its diagram. Kept for the reasoning behind ADR-033. Moved out of the route repository on 08/09/2026. |
| [code/MODO-CODE-20260709.md](code/MODO-CODE-20260709.md) | Spec do **Modo Code** (09/07/2026) — o desenho que a ponte nativa implementa. |
| [code/RUN-20260910.md](code/RUN-20260910.md) | **The Run in Code mode** (10/09/2026) — design, contracts and build plan for turns that continue by themselves: where a turn ends today (the eight answers), the three tiers (rule → orchestrator profile → human), the `stop_request` handshake, the `/orchestrate` endpoint, the run state machine, and the blocks to queue. Proposal; nothing implemented. Screen: [code/modo-code-run-mockup.html](code/modo-code-run-mockup.html). Decision: ADR-034. |
| [code/F0-mapa.md](code/F0-mapa.md) | **Histórico:** o mapeamento F0 do Modo Code, de 09/07/2026. O cabeçalho ainda diz *"aguardando ratificação"* — foi ratificado pelos fatos: a ponte existe desde a 1.1.x e passou pelos achados F-12, F-13 e F-15. Fica como registro do que se sabia antes de escrever o código. |

## Convenções de docs

- **Uma frase declarativa, depois código/lista** no topo de cada página. Sem
  "neste guia vamos explorar…".
- **Nomes de arquivo em kebab-case minúsculo.** Ordem vive neste índice, não no
  nome do arquivo.
- **Decisões vão em [decisoes.md](decisoes.md)** (ADR). Não relitigar direção já
  decidida dentro de um how-to — linkar o ADR.
- **Sem doc, sem deploy.** Função nova vira doc aqui antes de entrar.

## Mapa de repositórios

- **Este repo** — cliente desktop (Tauri 2).
- **ShvIA** (`/Users/samir/x/IA`) — servidor Laravel, fonte da verdade
  (`https://ai.shvia.org`).
- **SHVTERM** (`/Users/samir/Projetos/SHVTERM`) — base técnica (Tauri), repo irmão.
- **`archive/claude-fork`** — snapshot do fork Claude Desktop descartado.
