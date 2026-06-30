# ShvIA Desktop — Estado e Continuidade

> **Ler primeiro.** Notas de continuidade. Última atualização: **30/06/2026**.

## Onde estamos

Repo **recém-pivotado**: era o fork `claude-desktop-debian` (SHVIA-DESKTOP,
empacotamento do Claude Desktop p/ Linux); virou o **cliente desktop do ShvIA**.

**Hoje (30/06/2026) foi feito:**
- Decidida a arquitetura (ver [escopo-projeto.md](escopo-projeto.md) e
  [../docs/decisoes.md](../docs/decisoes.md)).
- **Fork arquivado** em `archive/claude-fork` + tag `archive/claude-fork-v0.2.2`,
  **empurrado ao `origin`** (rede de segurança — nada perdido, inclui o recolor
  indigo/navy que tinha sido feito no fork).
- Working tree do `master` **esvaziada** e **documentação de fundação criada**
  (README, CLAUDE/AGENTS, `.claude/`, `.continue/`, `docs/`, `version.md` 0.1.0).
- **Nenhum código de app ainda** — implementação começa amanhã pela Fase 1.

## Decisões travadas

1. **Base = SHVTERM** (Tauri 2 + React, multiplataforma, CI/updater/packaging
   prontos). Fork Claude **descartado** (arquivado).
2. **Arquitetura F1 = shell fino Tauri carregando o ShvIA web (Blade) remoto**
   (`https://ia.blue3.com.br`). "Mesmas funções" de graça; zero rewrite.
3. **Servidor remoto = fonte da verdade** (dados, senhas, permissões). Sem banco
   no cliente.
4. **Repo reaproveitado** (`samirhvbr/SHVIA-DESKTOP`, `master`), histórico mantido.
5. **Auth = cookie de sessão Sanctum same-origin** na F1 (login = tela do ShvIA).

## Próximos passos (amanhã — Fase 1)

> Passo-a-passo completo em [../docs/roteiro-fundacao.md](../docs/roteiro-fundacao.md).

1. **SMOKE-TEST #1 (fazer ANTES de tudo):** validar o **streaming SSE do `/chat`
   no Linux/WebKitGTK**. É o maior risco multiplataforma. Se passar, o caminho
   está livre; se falhar, considerar fallback Electron (ainda thin-shell).
2. `npm create tauri-app@latest` (Tauri 2); janela única apontando para URL
   configurável (default = `https://ia.blue3.com.br`).
3. Validar **login Breeze → sessão Sanctum no WebView** e a **persistência do
   cookie entre reinícios** do app.
4. Título + ícone ShvIA (copiar de `/Users/samir/x/IA/brand/`).
5. **Em paralelo (dia 1):** iniciar **procurement do cert EV Windows** — é o long
   pole de prazo (dias a semanas).

## Pendências / decisões em aberto (confirmar com o time)

- [ ] **Online-only é aceitável** como propriedade de produto? (toda a arquitetura
      fina depende disso). Ver [escopo](escopo-projeto.md#decisões-em-aberto).
- [ ] **Verba + dono** do cert EV Windows (~US$300–600/ano) e Apple Developer
      (US$99/ano), incl. rotação da chave do updater.
- [ ] **Funções idênticas ao web** ou haverá **telas desktop-only**? (idênticas →
      shell fino é perfeito; divergir → exige cliente React sobre `/api/v1`, F2+).
- [ ] **Sidecar Python na F1?** A análise indica que **não é necessário** na F1
      (auth é cookie same-origin). Confirmar se entra só na F2 (vault/SSO/ações
      nativas na API) ou se há razão pra antecipar.
- [ ] **URL de DEV** do ShvIA (além de produção `ia.blue3.com.br`) para testar.

## Notas

- O **workflow de design multi-agente** (30/06) gerou a recomendação completa;
  síntese integrada em [escopo-projeto.md](escopo-projeto.md) e
  [../docs/decisoes.md](../docs/decisoes.md). Dois pontos a **verificar em código**
  na F1 (o recon estruturado falhou em parte; os fatos vieram dos agentes de
  design/crítica): (a) middleware exato de auth de `/chat` em `routes/web.php`;
  (b) o trecho de streaming SSE em `public/js/app.js` (~linha 4003).
