# Documentação — ShvIA Desktop

Índice da documentação técnica **estável** do projeto. Trabalho em andamento
(WIP) vive em [`../.continue/`](../.continue/) e migra para cá quando amadurece.

## Páginas

| Documento | Conteúdo |
|-----------|----------|
| [decisoes.md](decisoes.md) | **ADRs** — decisões de arquitetura registradas (formato Architecture Decision Record). |
| [arquitetura.md](arquitetura.md) | Arquitetura técnica detalhada: camadas, fluxo de auth, CSP/IPC, build/empacotamento. |
| [roteiro-fundacao.md](roteiro-fundacao.md) | Passo-a-passo da fundação (F0/F1) — comandos concretos para iniciar. |

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
  (`https://ia.blue3.com.br`).
- **SHVTERM** (`/Users/samir/Projetos/SHVTERM`) — base técnica (Tauri), repo irmão.
- **`archive/claude-fork`** — snapshot do fork Claude Desktop descartado.
