# ShvIA Desktop (`shvia-desktop`)

[English (US)](README.md)

ShvIA Desktop é o cliente nativo **Tauri 2 + Rust** do ShvIA para macOS, Windows
e Linux. Ele abre a interface hospedada do ShvIA e acrescenta motores locais de
Code, diálogos nativos de arquivos, integração com o sistema e acesso controlado
a esta máquina. O servidor padrão é [ai.shvia.org](https://ai.shvia.org); o dono
pode configurar outro servidor ShvIA confiável. Documentação interna do projeto.

**Revisado em 15/10/2026 sobre a base de runtime 1.29.0.** A versão atual do
repositório está em [`version.md`](version.md). Esta visão geral descreve capacidades
implementadas, sem afirmar aceite de todas as plataformas ou versões instaladas.

Comece pelo [estado do projeto](docs/project-state.md), pela [revisão documental](docs/documentation-review-20261015.md)
e pelo [índice da documentação](docs/README.md). As convenções de desenvolvimento
estão em [AGENTS.md](AGENTS.md); decisões estão no [registro de ADRs](docs/decisoes.md).

## Produto e arquitetura

A janela principal carrega a própria interface Laravel/Blade do servidor. Chat,
projetos, bots, automações, conhecimento, reuniões, comparação, conta e administração
acompanham, portanto, o deploy Web configurado e as permissões da pessoa autenticada.
A casca local TypeScript/Vite fornece inicialização, retorno sobre a conexão,
configuração do servidor e telas de recuperação; ela não reimplementa o aplicativo Web.

O servidor mantém os dados da aplicação, as contas e as permissões. O desktop não
executa um banco local da aplicação. Ele mantém cookies da WebView, preferências
nativas, permissões de pastas, configuração de motores/contas e os registros locais
privados necessários ao fluxo de avaliações. Existe tela offline; chat com IA offline
e replicação do servidor não estão implementados.

A página remota usa uma ponte nativa delimitada, com verificação de origem, token
de capacidade da sessão, restrições de pasta do projeto e confirmação nativa onde
exigida. IPC genérico do Tauri e controle do updater não são expostos à página
remota. Escolher outro servidor é uma decisão de confiança: ele recebe as pontes
nativas suportadas pelo aplicativo. A sonda de conexão verifica alcance, não que
o site remoto seja uma instância ShvIA.

| Superfície de execução | Onde o trabalho roda | Credencial e contabilidade |
|---|---|---|
| Recursos Web dentro do aplicativo | Servidor ShvIA configurado e seus provedores | Login Web normal na mesma origem; permissões e contabilidade do servidor |
| Code local, motor gateway | `anna` nesta máquina, inferência pelo ShvIA | Credenciais curtas de sessão/delegadas; binário `anna` independente |
| Code local, motor Claude | `claude-runner` e Claude Agent SDK nesta máquina | Conta/assinatura Claude local selecionada; inferência direta ao provedor |
| Code local, motor Codex | `codex-runner` → `codex app-server --stdio` nesta máquina | Conta mantida pelo `codex login`; sua assinatura, sem injetar chave de API OpenAI |
| Code remoto | Serviço Workspace configurado pelo ShvIA Web | Projeto/sessão remotos vinculados à conta; arquivos e execução permanecem remotos |

Code local e remoto compartilham conceitos da interface Web, mas usam pontes,
sistemas de arquivos e fluxos de credenciais diferentes. Uma prévia de arquivo do
Workspace remoto não concede acesso ao sistema de arquivos local deste desktop.
Veja [Modo Code](docs/code/MODO-CODE-20260709.md) e [estado do projeto](docs/project-state.md)
para os limites atuais.

## Capacidades nativas implementadas

| Área | Capacidade atual | Detalhes |
|---|---|---|
| Janelas e sistema | Várias janelas, encaminhamento para instância única, geometria persistida, bandeja/menu, fechar para bandeja e iniciar com o sistema configuráveis | [Funcionalidades](docs/funcionalidades.md) |
| Acesso rápido | Chat rápido, atalho global selecionável e deep links `shvia://` permitidos | [Arquitetura](docs/arquitetura.md), ADR-038 |
| Notificações e atualização | Polling de notificações nativas, contagem nos ícones suportados, checagem automática/manual e confirmação antes de instalar | [Build e distribuição](docs/build.md), ADR-011/022/028 |
| Arquivos e ferramentas | Diálogos nativos para abrir/salvar, árvore/status/diff/leitura do projeto local, salvar artefatos, suporte a colar imagens e configurar clientes CLI | [Modo Code](docs/code/MODO-CODE-20260709.md), [arquitetura](docs/arquitetura.md) |
| Supervisão de Code | Escolha de motor/modelo/esforço, sessões por projeto, reanexação após reload, cartões de aprovação, Plano, orientação durante o turno e Run supervisionado | [Run](docs/code/RUN-20260910.md), [Codex](docs/code/MOTOR-CODEX-20260909.md) |
| Contas dos motores | Perfis e login Claude, catálogos consultados no runtime, leitura do uso dos planos; fontes dos runners no pacote e instalação/atualização | [Contas Claude](docs/code/CONTAS-CLAUDE.md), [uso dos planos](docs/code/USO-DAS-CONTAS.md) |
| Claude no Workspace remoto | Ponte nativa implementada: consentimento, `claude setup-token` em terminal privado e envio direto do token ao servidor configurado; exige página Web compatível para acioná-la | [Conexão da assinatura](docs/code/ASSINATURA-NO-WORKSPACE.md) |
| Aparelho e navegador do agente | Operações do aparelho ativadas pelo usuário, seletor de arquivos e captura de tela; navegador isolado opcional com confirmação das ações | [Funcionalidades](docs/funcionalidades.md), ADR-039/042 |
| Avaliações controladas | Consentimento e verificação nativa do runtime, controle/recuperação, revisão/reteste privados, histórico durável, sincronização de relatórios e visualizador local separado | [Autorização](docs/PENTEST/authorization.md), [compatibilidade](docs/pentest-compatibility.md), [visualizador](docs/pentest-viewer.md) |

A conexão da assinatura é uma capacidade nativa, sem comprovar que existe um botão
funcional em Configurações em todo deploy Web. A base Web revisada em 15/10/2026 ainda
não acionava essa ponte; sua integração e um login real exigem aceite separado.

As políticas de aprovação dos runners diferem. Claude possui política própria de
ferramentas e restrições de caminhos com segredos. Codex usa o conjunto de escrita
de seu sandbox; escritas nesse conjunto podem ocorrer sem cartão, e leituras de
segredos locais não recebem o mesmo filtro do Claude. Plano muda a política do
turno. O modo Codex com contêiner externo é explícito e delimitado ao diretório
configurado. Leia o [contrato do runner Codex](codex-runner/README.md) antes de
presumir que aprovações ou controles de privacidade são iguais entre os motores.

Motores de assinatura direta não enviam a inferência pelo gateway ShvIA: cota,
mascaramento de PII e contabilidade `inference_requests` do gateway não cobrem essas
chamadas. O uso do plano informado pelo cliente é uma medida diferente do gasto de
API do servidor.

## Limites por plataforma

| Plataforma | Implementação e limites |
|---|---|
| macOS | WKWebView; motores locais e ponte nativa; conexão de assinatura Claude e avaliação controlada usam a implementação Unix. Assinatura/notarização e aceite instalado são etapas separadas da release |
| Windows | WebView2; ponte nativa e instaladores PowerShell dos runners existem. Conexão de assinatura Claude e perfil de avaliação controlada não são suportados nesta versão. Contagem nativa no ícone não é suportada; use o [roteiro de validação Windows](docs/roteiro-validacao-windows.md) para o comportamento instalado |
| Linux | WebKitGTK 4.1; integração nativa de mídia/colar e TTS existe, com aceite de microfone e colagem real de imagem acompanhado no [roteiro de mídia](docs/roteiro-microfone-e-colar.md). No Wayland, associe um atalho do sistema a `xdg-open shvia://rapida`; não presuma que o aplicativo registra um atalho global |

Captura de tela depende da permissão/portal do sistema. O navegador do agente usa
perfil isolado, bloqueia pop-ups/downloads e pode não atender logins que dependam
deles. Uma VM/ambiente de GPU que mostre janela Linux em branco pode usar o ajuste
documentado `WEBKIT_DISABLE_DMABUF_RENDERER=1`. Código implementado por plataforma,
checagem de compilação e um fluxo real no aplicativo instalado são evidências distintas.

## Desenvolvimento a partir do código

O aplicativo usa Rust stable e Node **20.19+ ou 22.12+** para a cadeia Vite
(o CI usa Node 22). Instale os pré-requisitos Tauri da plataforma: ferramentas de
linha de comando do Xcode no macOS; ferramentas MSVC C++ e WebView2 no Windows;
WebKitGTK/GTK e bibliotecas de empacotamento no Linux. Os pacotes exatos estão em
[pré-requisitos de build](docs/build.md#pré-requisitos).

```bash
npm ci
npm run build
npm run prova:bump
npm run prova:politica
```

`npm run build` compila a casca local de inicialização, sem gerar instalador.
Para desenvolver o aplicativo nativo com o motor gateway, prepare antes um `anna`
compatível vindo do SHVIA-CODE:

```bash
npm run anna:stage -- --from /caminho/absoluto/para/anna
npm run tauri dev
```

Se nenhum `anna` será empacotado, substitua a declaração `externalBin` do Tauri;
um clone limpo não contém o binário preparado. Para desenvolver a casca/outros motores:

```bash
TAURI_CONFIG='{"bundle":{"externalBin":[]}}' npm run tauri dev
```

No PowerShell, defina `$env:TAURI_CONFIG` com o mesmo JSON antes de
`npm run tauri dev`. A configuração omite o motor gateway do pacote; um `anna`
externo instalado ainda pode ser encontrado em runtime. Nunca crie um motor falso
para distribuir uma release.

O runner Claude exige Node 18+ e as dependências do SDK; o runner Codex exige
Node 18+ e o CLI Codex instalado e autenticado. O aplicativo pode instalar/atualizar
runners a partir das fontes incluídas no pacote; veja [runner Claude](claude-runner/README.md)
e [runner Codex](codex-runner/README.md). A arquitetura atual não prevê um sidecar
Python/PyInstaller. Ferramentas opcionais e runtimes de avaliação têm dependências próprias.

As verificações Rust rodam em `src-tauri` (`cargo test --locked`,
`cargo clippy --locked --all-targets -- -D warnings`), com a mesma preparação do
motor ou substituição `TAURI_CONFIG`. O [CI](.github/workflows/ci.yml) também roda
verificações de runners, instaladores, versão, dependências e publicação. Passar
nelas não empacota, assina, publica nem instala uma release.

## Versão, build e distribuição

[`version.md`](version.md) é a versão do desktop, independente do ShvIA Web e dos
motores. `npm run version:sync` propaga para manifests/lockfiles;
`npm run prova:bump` verifica os portadores. A mudança registra versão e changelog
no mesmo commit. Assunto e cabeçalho de changelog seguem **`X.Y.Z - descrição em
inglês (US)`**. Z cobre mudanças visíveis/empacotamento/build; Y cobre novas
capacidades de runtime, IPC ou auth-handoff; X é uma mudança manual para versão estável.

O aviso de compatibilidade compara o **BUILD** deste desktop instalado com
`clients.desktop.min_version`, recebido de `GET /api/v1/version` autenticado
(com fallback para o payload antigo de health). Ele avisa, pode ser dispensado e
não bloqueia o aplicativo. Não é uma exigência de versão mínima do servidor;
`latest_version` isolado não dispara o aviso. Capacidades nativas individuais e
compatibilidade do runtime de avaliação possuem verificações próprias.

O empacotamento de release é local, em cada sistema de destino: `build-local.sh`
para macOS/Linux, `build-local.ps1` ou `build-local.cmd` para Windows. As saídas
incluem DMG/arquivo do app, MSI/NSIS, AppImage/DEB/RPM e o fluxo separado de pacote
Arch. O usuário recebe instaladores/manifests publicados no servidor de distribuição
configurado; GitHub Releases registra a versão e os artefatos publicados. No Arch,
a atualização usa pacman. Veja [build, assinatura e publicação](docs/build.md) para
credenciais, verificações, checksums e opções de publicação.

Release do repositório, instalador empacotado, assinaturas/notarização válidas,
publicação no servidor e aceite no aplicativo instalado são resultados separados.
Informe cada um explicitamente; commit novo ou CI verde não atualiza o aplicativo
instalado de ninguém.

## Repositório e identidade

| Caminho | Responsabilidade |
|---|---|
| `src/`, `index.html` | Casca local de inicialização e conexão TypeScript/Vite |
| `src-tauri/src/` | Janelas, ponte, motores, integração com SO e ciclo das avaliações em Rust |
| `claude-runner/`, `codex-runner/` | Runners Node, módulos de protocolo/política e instaladores |
| `scripts/`, `build-local.*`, `packaging/` | Preparação, validação, empacotamento e publicação |
| `src-tauri/icons/`, `brand/`, `public/` | Exports nativos e ativos Shield |
| `docs/` | Contratos atuais, runbooks e decisões |
| `.continue/` | Trabalho/design pendente; sua existência não comprova implementação |

**Shield** é a identidade ShvIA ativa: escudo grafite/titânio, hemisfério esquerdo
cobalto e nós de orquestração. Mantenha ShvIA distinta da identidade corporativa
Blue3. As [fontes de marca](brand/README.md) são canônicas neste checkout;
`brand/versao2-portal/` preserva a arte anterior. SHVTERM é um projeto irmão separado
cujos padrões desktop ajudaram a formar este cliente; o fork descartado de Claude
Desktop permanece histórico, sem representar a arquitetura atual.
