# ShvIA Desktop documentation

Start with the current capability map, then choose the contract or operator check for the
task. The 2026-10-15 review describes source, not installed-device acceptance. Work not yet
produced stays in [`.continue/`](../.continue/); historical designs and dated measurements
remain evidence of the decisions they led to.

## Current product and architecture

| Document | What it answers |
|---|---|
| [project-state.md](project-state.md) | What Desktop does today, Web/Workspace boundaries, requirements, platform support and acceptance limits |
| [documentation-review-20261015.md](documentation-review-20261015.md) | What the documentation review corrected, what remains uncertain and what needs a separate decision |
| [funcionalidades.md](funcionalidades.md) | Current capability summary and the chronological implementation record; old behavior is labeled when superseded |
| [arquitetura.md](arquitetura.md) | Shared Web UI, native bridge, trust boundary, local/remote sessions, persistence, updater and assessment architecture |
| [build.md](build.md) | Development and local packaging/signing/publication; CI checks and release/deployment are separate stages |
| [brand.md](brand.md) | Shield identity and canonical desktop sources; changing an asset does not distribute an installed build |
| [decisoes.md](decisoes.md) | Architecture decision records and the measured reasons behind them |

## Code engines, accounts and remote work

| Document | What it answers |
|---|---|
| [code/CONTAS-CLAUDE.md](code/CONTAS-CLAUDE.md) | Local Claude profile registry, UI registration, account variables, child-process resolution and historical Linux/macOS measurements |
| [code/USO-DAS-CONTAS.md](code/USO-DAS-CONTAS.md) | Claude/Codex plan-limit reads, percentage used, cache isolation and unknown readings |
| [code/ASSINATURA-NO-WORKSPACE.md](code/ASSINATURA-NO-WORKSPACE.md) | Native half of connecting a Claude subscription to ShvIA: Unix PTY, consent and private delivery; requires a compatible Web caller, separate from the existing manual token form |
| [code/MOTOR-CLAUDE-DIAGNOSTICO.md](code/MOTOR-CLAUDE-DIAGNOSTICO.md) | Separate runner, official-client login and profile problems when local Claude does not answer |
| [code/MOTOR-CODEX-20260909.md](code/MOTOR-CODEX-20260909.md) | Codex runner protocol and sandbox proof, with dated measurements and the later external-container sandbox contract; the Desktop bridge and model selector are implemented |
| [code/RUN-20260910.md](code/RUN-20260910.md) | Run design and implemented contracts: rules, orchestrator, held stop, caps and UI. Its dated status records B0–B8 delivered, with B9's real-run measurement separate |
| [code/RUN-ORCHESTRATOR.md](code/RUN-ORCHESTRATOR.md) | B9 operator protocol: five real runs across three engines and a sheet for false continues/unnecessary asks; a prepared protocol is not an executed result |
| [Web Remote Code](https://github.com/samirhvbr/shvia-web/blob/master/docs/FRONTEND/REMOTE-CODE.md) | Shared screen, remote project creation, screen takeover, files/diffs and approved GitHub publication; owned by the Web repository |

The Desktop's local Claude profiles and the Web's remote subscription connection are different
credentials. Codex's runner can support an external container boundary without establishing a
shipped remote Codex option in ShvIA Web. Refer to each component's current capability map.

## Controlled assessment

| Document | What it answers |
|---|---|
| [PENTEST/authorization.md](PENTEST/authorization.md) | Native authorization, local process supervision, recovery, telemetry, review/retest and private report/history synchronization |
| [pentest-compatibility.md](pentest-compatibility.md) | Installed runtime preflight, Unix support, fork-version check and reviewed version pairs |
| [pentest-viewer.md](pentest-viewer.md) | Viewer in the OS browser, native consent, private session capability and lifecycle |

## Acceptance and historical foundations

| Document | Scope |
|---|---|
| [roteiro-validacao-windows.md](roteiro-validacao-windows.md) | Windows installed-app validation protocol, expected results and report table; read its dated findings separately from later implementations |
| [roteiro-microfone-e-colar.md](roteiro-microfone-e-colar.md) | Linux microphone/image-paste checks that need the person and real OS input; WebKit capability discovery alone does not prove capture/paste |
| [roteiro-fundacao.md](roteiro-fundacao.md) | Historical F0/F1 foundation commands; use the current build guide for today's setup |
| [code/MODO-CODE-20260709.md](code/MODO-CODE-20260709.md) | Original Code-mode specification, retained for protocol/design history |
| [code/F0-mapa.md](code/F0-mapa.md) | Historical discovery before the native Code bridge; its pending-ratification wording is not current implementation status |
| [code/CONTAS-CLAUDE-proposta-20260905.md](code/CONTAS-CLAUDE-proposta-20260905.md) | Archived proposal behind ADR-033, with its diagram; the active profile contract is CONTAS-CLAUDE.md |

## Documentation conventions

- Begin with the document's purpose and current scope. Label proposed work, dated observations
  and installed acceptance separately from implementation.
- New filenames use lowercase kebab-case. Existing names stay stable so links keep working.
- Record decisions in [decisoes.md](decisoes.md), then link them from the operational contract.
- A changed feature carries its documentation in the same delivery; update this index when a
  document is added. Keep known limits beside the capability they qualify.

## Repository boundaries

This repository owns the Tauri client, Rust bridge and local runners.
[ShvIA Web](https://github.com/samirhvbr/shvia-web) owns Laravel and the shared interface;
[ShvIA Workspace](https://github.com/samirhvbr/shvia-workspace) owns remote repositories and
execution. SHVTERM was the technical foundation, not a merged application. The discarded
Claude Desktop fork remains historical under `archive/claude-fork`.
