# Changelog

Entradas no formato da mensagem de commit (`versão - comentário`, AGENTS.md),
mais recente primeiro. É daqui que a skill COMMITTER tira a mensagem (AGENTS.md §PS).

## 1.3.2 - Nota do desligamento do COMMITTER passa a citar o T5

- Correção de revisão do dono: a nota do `.committer.yml` aponta o defeito medido (scan de
  segredo des-stageando por assunto, 5/6 falso-positivo na 2.92.0) e deixa claro que desligar
  tira o gatilho, não conserta o ADR-005.
- Bump Z: os 6 portadores ressincronizados via `npm run version:sync` (mantê-los em dia evita
  reabrir a divergência que a 1.3.0 corrigiu).

## 1.3.1 - Desliga o COMMITTER (kill-switch, a pedido do dono)

- `enabled: false` no `.committer.yml` — o kill-switch da SPEC §1.2, sem apagar o marcador.
- Desligado em toda a casa `x/SHVIA/*` em 29/08/2026: passou a haver agente mandando PR, e
  commit automático concorrendo com PR embaralha a história. Commit e push voltam a ser do
  agente da sessão.
- Bump Z: os 6 portadores de versão foram ressincronizados via `npm run version:sync` (a
  1.3.0 já havia corrigido a divergência deles; manter em dia evita reabri-la).

## 1.3.0 - A ponte ganha `readFile`: a árvore da aba "Arquivos" passa a poder abrir a prévia de um arquivo

- **Nova capacidade de runtime** (bump Y): `window.__shviaCode.readFile(path, file)`, no molde do
  `gitDiff` ao lado. → `{ok, content, truncated, binary, bytes}`. 5 testes contra o FILESYSTEM de
  verdade num diretório temporário — como o do `gitDiff`, um mock provaria que sabemos montar
  argumentos, não que o SO devolve o que esperamos.
- **Por que existe:** no SHVIA-WEB a aba "Arquivos" do painel do Modo Code lista a árvore, mas não
  tinha como mostrar o conteúdo de um arquivo ao clique. Era o próximo passo cross-repo para
  tornar as abas produtivas (pedido do Samir, 26/08). A aba "Alterações" já abre o diff ao clique
  (1.2.0); esta é a irmã para os arquivos que ainda não mudaram.
- ⚠️ **Não foi pedir ao `anna`**, que sabe ler arquivo — seria uma inferência PAGA para preencher
  um painel que se atualiza sozinho ao voltar o foco. Prévia é leitura de estado; o precedente é
  o `git_diff`, não o agente.
- 🔴 **A cerca é EXPLÍCITA:** canonicaliza a pasta e o alvo e exige que o alvo esteja DENTRO da
  pasta do projeto. O nome vem do `list_tree` do mesmo host (confiável), mas ler arquivo é mais
  perigoso que listar — um `..` ou symlink que escapasse é recusado antes da leitura. Reversão
  provada: sem o `starts_with`, o teste que lê um arquivo fora da pasta passa a vazar.
- **Binário não vira texto vazio:** NUL nos primeiros 8 KB (heurístico do git) → `binary:true`
  com content vazio e o `bytes` real, para a página dizer o tamanho sem despejar bytes de um PNG.
  **Arquivo grande é cortado e o diz** (`truncated`) — é prévia, não editor; `from_utf8_lossy`
  resolve o multibyte partido na fronteira sem pânico.
- Consumo no web: SHVIA-WEB (a árvore da aba Arquivos abre a gaveta de prévia). Enquanto o app
  instalado não tiver esta versão, a página degrada como "casca velha" — a linha não fica
  clicável e a aba avisa para atualizar o app, no mesmo padrão do `gitDiff`.

## 1.2.0 - A ponte ganha `gitDiff`: a aba "Alterações" do painel passa a ter o que abrir

- **Nova capacidade de runtime** (bump Y): `window.__shviaCode.gitDiff(path, file)`, no molde
  do `gitStatus` ao lado. 6 testes contra um repositório git **de verdade** num diretório
  temporário — um mock do `Command` provaria que sabemos montar argumentos, não que o git
  entende os que montamos.
- **Por que existe:** o SHVIA-WEB `2.110.39` transformou o painel do Modo Code em abas, e a
  aba **Alterações** lista os arquivos alterados mas não tinha como mostrar o diff de um.
  Era o único item cross-repo da frente. Ver `docs/FRONTEND/PAINEL-CODE-EM-ABAS.md` lá.
- ⚠️ **Não foi pedir ao `anna`**, que tem a ferramenta `git_diff`. Seria uma **inferência paga
  para preencher um painel** — e o painel se atualiza sozinho ao voltar o foco da janela,
  então cada alt-tab viraria uma chamada de modelo.
- **`vazio ≠ sem mudança`:** `git diff` compara a árvore contra o ÍNDICE, então um arquivo já
  preparado (`git add`) devolve vazio. A função cai no `--staged` e devolve `staged: true` —
  sem isso o painel diria "sem alterações" para quem acabou de ver o arquivo listado como
  alterado.
- **Teto de 256 KB cortado em fronteira de CARACTERE.** O painel pinta o diff linha a linha no
  DOM, e um lockfile ou bundle chega a megabytes. `texto[..N]` em UTF-8 entra em pânico no
  meio de um multibyte, e diff em português tem acento em toda linha.
- **O `--` antes do caminho** impede que um arquivo chamado `-p` (ou homônimo de branch) seja
  lido pelo git como opção ou revisão. Há teste com um arquivo `-p` de verdade.
- 🟡 **Um teste que passava por SORTE foi consertado antes de contar como prova.** A primeira
  versão do teste de truncamento usava um único comprimento de linha, e o corte em 256 KB caía
  numa fronteira de caractere por acaso — a reversão (voltar ao slice cru) **passava**. Um
  teste que só falha com sorte não prova guarda nenhuma. Passou a variar o deslocamento byte a
  byte, e aí a reversão estoura com `end byte index 262144 is not a char boundary; it is
  inside 'ã'`.

## 1.1.36 - Corrige o retrato da 1.1.35: as duas plataformas JÁ estão publicadas na 1.1.34

- A 1.1.35 registrou como pendência aberta ("o macOS está sem caminho de
  atualização") algo que **foi resolvido no mesmo dia, poucas horas depois**: o
  manifesto está em **1.1.34 com `linux` e `macos`**, e o app instalado é o
  1.1.34. Escrevi o retrato com a medição de 19h30 e não remedi antes de commitar.
- **Documentação errada é pior que documentação nenhuma** — alguém leria a
  pendência 0 e ia publicar de novo, ou pior, ia achar que a entrega da 1.1.34
  não chegou a ninguém.
- **O que ficou** (e é o que importava): a regra de que o `release-manifest.mjs`
  recomeça o manifesto quando a VERSÃO muda e só mescla plataformas da mesma
  versão. Publicar de uma máquina só, depois de bumpar, apaga a outra plataforma.
  A ordem certa é: publica numa, publica na outra, **sem bumpar no meio**.

## 1.1.35 - Registra o buraco de atualização do macOS: manifesto publicado está na 1.1.29 e só com linux

- **Retrato, não conserto.** O manifesto em `ai.shvia.org/storage/desktop/release.json`
  está na **1.1.29 com `linux` apenas**, enquanto o repo está na 1.1.34 — quem usa macOS
  não recebe nem o aviso de versão nova.
- **A causa é desenho, e precisa estar escrita:** o `release-manifest.mjs` recomeça o
  manifesto quando a VERSÃO muda e só mescla plataformas da mesma versão. Um build
  publicado do Arch sozinho apaga os artefatos de macOS da versão anterior. Para o
  manifesto ter as duas, **as duas máquinas têm de buildar a mesma versão**, sem bumpar
  entre uma e outra.
- Entrou como **pendência 0** (a primeira da lista) em `.continue/estado-atual.md`, com a
  ordem dos dois comandos e o que a 1.1.34 carrega e ainda não chegou a ninguém no
  macOS: o PATH do shell de login para os sidecars (ADR-030), o wrapper do
  `claude-runner` com caminho absoluto do node, e o `anna` 0.11.x.

## 1.1.34 - O wrapper do claude-runner grava o caminho do node: "claude-runner não encontrado" era o node faltando

- **O sintoma culpava o binário errado.** O motor Claude Code não subia com
  "claude-runner não encontrado — rode claude-runner/install.sh", mas o runner
  estava instalado: o wrapper `~/.local/bin/claude-runner` fazia `exec node …`
  contando com o PATH, e app de GUI não herda PATH de shell (ADR-030). Dentro do
  app o wrapper morria com `node: not found`, e a página traduzia para "não
  encontrado" — mandando o usuário reinstalar o que já estava lá (caso real de
  20/08, com o runner recém-instalado e funcionando pelo terminal).
- `claude-runner/install.sh` grava o **caminho absoluto do node** no wrapper, com
  fallback para o PATH se o node mudar de lugar depois (upgrade do Homebrew,
  troca de gerenciador de versão). Provado nos dois sentidos: com o PATH mínimo
  do launchd (`/usr/bin:/bin:/usr/sbin:/sbin`) o wrapper antigo falha e o novo
  roda; e um turno real pela assinatura respondeu (`claude-opus-5[1m]`).
- O `user_env.rs` (1.1.24) já resolvia o PATH do processo do sidecar; este
  conserto cobre o wrapper, que é um processo `sh` à parte e não passava por lá.

## 1.1.33 - O runner passa a responder --version, e o package.json dele deixa de ser um número parado

- ⚠️ **A 1.1.32 expôs uma sonda que devolveria lixo.** O `engine_status()` roda o
  binário com `--version` e trata a saída como o NÚMERO. O `anna` responde; o
  `claude-runner` **ignorava a flag** — subia em modo host, recebia EOF no stdin e
  saía, devolvendo NDJSON de arranque que a ponte mostraria como se fosse a
  versão. Sonda que responde qualquer coisa é pior que sonda que não responde:
  "encontrado, mas não respondeu" o desktop sabe classificar; lixo exibido como
  fato, não.
- **`claude-runner/package.json` vira PORTADOR de versão** (`sync-version.mjs`,
  6 portadores agora). Ele dizia `0.1.0` desde que nasceu — número parado que não
  respondia à única pergunta que se faz dele: *qual app trouxe este runner?*
  Portador que não é sincronizado é pior que portador nenhum, porque **parece**
  resposta.
- **O import do SDK virou dinâmico**, depois do `--version`. Com o `import`
  estático, a resolução do pacote acontecia antes de qualquer linha nossa rodar:
  numa instalação sem `npm install` o runner morria com `ERR_MODULE_NOT_FOUND` e
  o desktop lia isso como "binário corrompido", quando o diagnóstico certo é
  "está lá, faltam as dependências". Agora `--version` responde **mesmo sem o
  SDK** — que é exatamente quando o diagnóstico é mais útil.
- **3 réguas novas** (8+19+3 = 30 no `prova:runner`), com o subprocesso rodando
  neste repo **sem `npm install`**, de propósito: é assim que se prova que o
  `--version` não depende do SDK.
- 🐛 **Uma reversão voltou VERDE e virou comentário.** A régua "reporta a versão"
  lê o `package.json` do runner e compara — então dessincronizar os dois **juntos**
  passa. Ela prova que o runner reporta o próprio portador, não que o portador
  está em dia; quem prova a sincronia é o `npm run prova:bump`, onde a mesma
  reversão derruba o build nomeando o arquivo. Está escrito lá: duas provas, cada
  uma com a sua metade.

## 1.1.32 - A tradução de evento do runner ganha 19 réguas, e o engineStatus deixa de ser braço inalcançável

- ⚠️ **A parte que mais importa:** `traduzirMensagem` foi extraída como função
  PURA e provada. Era o trecho mais perigoso do runner e o único sem régua
  nenhuma — quando a forma de um evento do SDK muda, **nenhum `case` casa, nada é
  emitido e nada falha**. A tela do Modo Code emudece e o turno "termina" sem uma
  linha: sem exceção, sem log, sem vermelho em lugar nenhum. O defeito perfeito.
- **19 réguas novas** (`npm run prova:runner`, 8+19 = 27), sobre a função **real**
  extraída do arquivo em produção. Cada uma fixa a forma exata de um evento:
  - `init` guarda o `sessionId` — perdê-lo reinicia a conversa em silêncio, com o
    modelo respondendo do zero;
  - `system` que não é `init` é silencioso — senão a chip do modelo pisca a cada
    mensagem de serviço;
  - só `text_delta` vira texto: `thinking_delta` na tela seria raciocínio
    vazando como resposta;
  - o bloco `assistant` **não repete** o texto quando já houve deltas — repetir
    duplicaria a resposta;
  - `tool_result` com content em ARRAY vira JSON, não `[object Object]`;
  - `result` **sempre** fecha com `turn_done`, **inclusive no erro** — sem ele a
    interface fica presa em "pensando" para sempre;
  - tipo desconhecido e mensagem nula são ignorados sem estourar: SDK novo manda
    tipos que este runner não conhece, e derrubar o turno por isso trocaria uma
    funcionalidade que falta por uma sessão perdida.
- **Sete reversões com controle**, cada uma acendendo só a sua régua.
- 📌 **`engineStatus` deixa de ser inalcançável.** O Rust tratava a ação desde a
  1.0.0 e o wrapper JS **nunca a expôs** — e o comentário ao lado dizia que "a
  página pergunta antes de oferecer o Modo Code". Não perguntava, e não tinha
  como. Pior: o comentário estava colado no `writeCliConfig`, então lia como se
  fosse dele. Exposto agora, com o consumidor real sendo a versão do motor à
  vista de quem for pedir suporte.
- ⚠️ **E ela NÃO é gate de capacidade** — para isso continua valendo `recursos`.
  A ação só existe em cascas que já a expõem, então usá-la como gate responderia
  sempre "sim", que é o contrário do que um gate precisa fazer.

## 1.1.31 - A ponte passa a declarar o que esta casca sabe fazer, e o piso do anna sobe para 0.11.4

- **`window.__shviaCode.recursos = { imagem: true }`** — constante local da ponte,
  sem ida ao Rust. A pergunta é "esta versão do app sabe fazer X?", e a resposta
  está na própria casca.
- ⚠️ **Por que isto existe, e por que era um furo da 1.1.30.** A página do Modo
  Code vem do **servidor** e atualiza a cada deploy; o `anna` e o `claude-runner`
  vêm **embutidos no app instalado**. Os dois andam em ritmos diferentes, então
  página nova conversando com casca velha é o estado NORMAL, não a exceção. Sem
  este flag, a página mandaria `images` no payload e uma casca anterior — que só
  lê `text` — descartaria a figura em **silêncio**: o chip na tela dizendo que
  foi, o modelo respondendo sem ter visto nada. A 1.1.30 entregou imagem no
  runner sem nada que dissesse à página se a casca a tinha; o furo estava aberto
  entre o deploy da página e a atualização do app.
- **O teste é de PRESENÇA, não de número de versão.** Ausência do flag é a
  resposta "não" — que é exatamente o que se quer de uma casca que não sabe
  responder. Comparar versão pediria à página que mantivesse a tabela de quem
  trouxe o quê.
- **`ANNA_MINIMO` sobe de `0.10.0` para `0.11.4`.** O flag é uma PROMESSA sobre os
  sidecars empacotados; empacotar um `anna` anterior faria a casca prometer o que
  o motor não cumpre. Piso baixo aqui não entrega Modo Code quebrado — entrega
  Modo Code **mentindo**, que é pior, e por isso derruba o build.
- 📌 **Medido de passagem, não consertado:** o Rust trata a ação de ponte
  `engineStatus` (`code_bridge.rs:528`) e o comentário ao lado diz que "a página
  pergunta antes de oferecer o Modo Code" — mas o **wrapper JS não a expõe** e
  nenhuma página a chama. Braço implementado e inalcançável, com um comentário
  descrevendo um consumidor que não existe.
- **Do outro lado:** `SHVIA-CODE` 0.11.4 e `SHVIA-WEB` 2.102.39.

## 1.1.30 - O runner passa a aceitar imagem no turno, em blocos do SDK, e ganha a primeira prova de 367 linhas sem nenhuma

- **O que entra:** `{"type":"user","text":"...","images":[{mime,dataBase64}]}`. O
  campo é **opcional** — sem ele o turno continua exatamente como estava. É a
  metade desktop do anexo do Modo Code (`SHVIA-WEB` 2.102.38); o cliente está
  documentado em `docs/FRONTEND/ANEXO-NO-MODO-CODE.md` daquele repo.
- **A ponte Tauri não mudou uma linha.** `fn send()` (`code_bridge.rs:704`)
  serializa o payload inteiro para o stdin, então campo novo atravessa sozinho. O
  dimensionamento inicial apontava para `code_bridge.rs:82` — que é o *comentário*
  do wrapper JS, não Rust. Medir encolheu a fatia.
- **`montarPrompt(text, images)`:** sem imagem devolve a **string** de sempre;
  com imagem, um `AsyncIterable<SDKUserMessage>` de um item. Cabe porque o runner
  já cria um `query()` **por turno** com `resume: sessionId` — não é sessão de
  streaming, é um turno que por acaso aceita iterável. Trocar todo turno de texto
  por iterável "para uniformizar" mudaria o caminho de 100% dos pedidos por causa
  de um caso que pode não acontecer.
- **Ordem dos blocos: imagem ANTES do texto.** A última coisa que o modelo lê é o
  que se está pedindo — mesma escolha do anexo de arquivo no cliente.
- **A fila deixou de guardar string.** `queue.push(String(msg.text))` perderia a
  imagem: o turno enfileirado sairia depois sem os blocos, **bem-formado e sem a
  figura** — silêncio com cara de sucesso. Guarda `{text, images}`, e a imagem
  viaja presa ao pedido que a trouxe. O mesmo defeito existia no `codeQueue` do
  cliente e foi consertado lá na mesma fatia.
- **Texto em branco não vira bloco vazio:** o SDK recusa `text: ""`, e imagem
  colada sem pedido ("olha isto") é um pedido legítimo.
- ⚠️ **A lacuna que esta versão fecha em parte:** o `claude-runner.mjs` tem 367
  linhas e **nenhuma prova** — é a metade do Modo Code que roda fora do gateway,
  com a assinatura do dono, e a única verificação era abrir o app e olhar. Nasce
  `npm run prova:runner` (`scripts/prova-montar-prompt.mjs`), no molde do
  `prova:bump`: 8 réguas sobre a função **real**, extraída do arquivo em produção.
  Conferidas por reversão — os três defeitos citados no cabeçalho dela foram
  reintroduzidos um a um, e cada um acendeu só a sua régua. O resto do arquivo
  segue sem prova; está registrado, não resolvido.

## 1.1.29 - O seletor do Modo Code passa a oferecer o Opus 5: o catálogo vinha do SDK, mas o SDK estava congelado pelo lock da instalação

- **O sintoma:** escolher **Opus** no dropdown dava Opus 4.8 (`/model` na sessão
  respondia `Current model: Opus 4.8 (1M context)`), e o Opus 5 não aparecia em
  lugar nenhum da lista — não havia como selecioná-lo sem digitar o ID inteiro.
- **O seletor não estava mentindo** (isso foi consertado na 1.1.28): ele mostra
  fielmente o `supportedModels()` do Agent SDK. Com o SDK **0.3.218** instalado,
  o catálogo tinha 5 linhas e o alias `opus[1m]` resolvia para
  `claude-opus-4-8[1m]`. Não havia Opus 5 para oferecer.
- **Onde o catálogo envelheceu: no `package-lock.json` do destino.** O
  `install.sh` copia o `package.json` (`^0.3.218`) e roda `npm install`, mas o
  lock de uma instalação anterior sobrevive em `~/.local/share/shvia-claude-runner`
  e ganha do `^` — reinstalar não atualizava nada. A 1.1.28 tirou o catálogo da
  casa para ele não envelhecer calado; a cópia que sobrou envelhecendo era o
  próprio SDK.
- **Correção:** piso em `^0.3.239` e `rm -f "$DEST/package-lock.json"` antes do
  `npm install`, para o `^` resolver de verdade a cada reinstalação. Não há build
  reproduzível a proteger aqui — o runner **não viaja no instalador** (só o `anna`
  viaja), é instalação local do dono da máquina.
- **O `install.sh` passa a imprimir a versão do Agent SDK** no fim. Ela **é** o
  catálogo: quando o seletor não oferece um modelo que já existe, é esse número
  que responde por quê. Mesmo motivo do `versao_do_anna` na 1.1.25 — diagnóstico
  escondido dentro de um `node_modules` é diagnóstico que ninguém faz.
- Catálogo depois de atualizar (medido em 21/08): `Default (recommended)` e
  `Opus (1M context)` → `claude-opus-5[1m]`; `Fable` → `claude-fable-5`;
  `Sonnet` → `claude-sonnet-5`; `Haiku` → `claude-haiku-4-5`. **Nenhuma linha de
  UI mudou** — o dropdown já lia o catálogo do motor.
- ⚠️ **Quem já tem o runner instalado precisa rodar `claude-runner/install.sh`
  de novo.** Sem isso o app segue com o SDK velho e o Opus 5 não aparece.

## 1.1.28 - Os seletores do Modo Code param de mentir no motor Claude: catálogo vem do SDK, e model/effort/aprovação chegam ao runner

- **O defeito:** com o motor **Claude Code** ativo, INFRA/MODELO/ESFORÇO seguiam
  mostrando o catálogo do **gateway** (`Kilo Gateway`, `openai/gpt-5.6-sol`) —
  nomes que não significam nada para o Claude Code. Aceitavam clique e não
  faziam efeito: a ponte lia `model`/`effort` e **descartava**. Seletor que
  parece funcionar é pior que seletor ausente, porque ninguém procura o defeito.
- **`claude-runner --modelos`** pergunta o catálogo ao próprio Agent SDK
  (`supportedModels()`) e imprime JSON. Cada linha traz `supportsEffort` e
  `supportedEffortLevels`, então a UI oferece só os níveis que aquele modelo
  aceita — e desabilita quando não aceita. Medido em 21/08: a chamada é de canal
  de controle e **não consome turno**. Perguntar ao SDK em vez de manter cópia é
  deliberado: os aliases (`opus[1m]`, `opusplan`, `best`…) mudam com o cliente.
- **`--effort` e `--aprovacao`** entraram no runner; a ponte repassa os dois.
- 🔒 **A trava que impede a correção de virar regressão:** a ponte só repassa
  `model`/`effort` quando a página manda `modelDoClaude: true`. Até agora o
  modelo era descartado, então a UI mandando um id do gateway era inofensivo;
  repassar sem conferir trocaria "seletor inerte" por "sessão que não abre".
  A garantia é declarativa — nada de adivinhar pela forma do id, que erraria no
  primeiro alias novo. Quem sabe de qual catálogo o valor saiu é quem montou o
  seletor.
- **Aprovação: os níveis continuam Manual/Edit/Auto nos dois motores**, e isso é
  decisão, não preguiça. O `permissionMode` do SDK **não estava em jogo** — quem
  decide é o hook `PreToolUse` do runner, que é o que faz o cartão de aprovação
  aparecer na tela do ShvIA. Passar o modo da Anthropic moveria a decisão para
  dentro do Claude Code, que a casca não renderiza: o usuário **perderia** a tela
  de aprovação em vez de ganhar controle. O hook agora honra os três níveis.
  `bypassPermissions` e `dontAsk` ficam de fora — não são "auto", são *sem gate*,
  e um motor não pode ser a porta dos fundos do outro ("não existe modo yolo").

## 1.1.27 - Reempacota o anna 0.11.3, e o número do sidecar passa a ser lido do artefato

- **O que muda no bundle:** o `anna` empacotado sai de **0.11.1** para **0.11.3**.
  Correção de premissa registrada: a defasagem **não** era o 0.8.4 de julho — os
  rebuilds de 19/08 (1.1.23–1.1.25) já haviam consertado aquilo. Medido em 20/08, o
  app instalado carregava 0.11.1 (`/usr/bin/anna`, mesma data do executável), então a
  distância real era **uma patch**.
- **O bloqueio que apareceu antes do build, e valeu mais que o build:** o `SHVIA-CODE`
  estava com o bump da 0.11.2 **pela metade** — `version.md` em 0.11.2 e `Cargo.toml`
  em 0.11.1 —, então `cargo build` produzia um binário respondendo **`anna 0.11.1` com
  o código da 0.11.2**. Stagear assim faria o *"Sobre → Motor do Code"* desta 1.1.25
  mostrar o número **errado**, que é pior que não mostrar. Consertado em SHVIA-CODE
  **0.11.3**, que também ganhou o `build.rs` que aquele repo não tinha.
- **O piso de versão NÃO dispararia, e está certo.** `ANNA_MINIMO = [0,10,0]`, e tanto
  0.11.1 como 0.11.3 estão acima — ele guarda contra binário **antigo**, não contra
  binário **mal etiquetado**. O caso de 20/08 não era do piso; era do bump.
- **A versão do sidecar foi conferida no BINÁRIO staged, não no log do
  `stage-anna`** — `sha256` igual ao compilado, `--version` lido do arquivo em
  `src-tauri/binaries/`. Log sem leitor foi exatamente o mecanismo que deixou o 0.8.4
  passar quatro semanas; o número tem de vir de onde ele vai rodar.

## 1.1.26 - O X pergunta antes de encerrar sessão do Code, e o app passa a ser uma instância só

- **O caso (20/08):** o X foi apertado sem querer com o Modo Code aberto, e o app
  fechou. Medido antes de mexer, e a medição **contradisse o sintoma**:
  `tray.json` diz `{"avisou":true,"close_to_tray":true}` — e não é reescrito desde
  30/jul —, então o `CloseRequested` deveria ter **recolhido** para a bandeja, não
  fechado. Ou seja: havia um defeito **antes** da confirmação.
- **A causa provável, e ela não era o diálogo que faltava:** havia **dois**
  `shvia-desktop` vivos ao mesmo tempo (um de 19/08 14:08, outro de 20/08 08:16,
  com pais diferentes) mais um zumbi desde 06/08 — e **não existia guarda de
  instância única**. Sem ela, cada invocação é um app novo, com bandeja própria e
  contagem de janelas própria, e aí o `webview_windows().len() <= 1` do
  `CloseRequested` decide **certo sobre a instância errada**: cada uma acha que é a
  única do mundo.
- **`tauri-plugin-single-instance`** entra, e **como primeiro plugin** do builder —
  é requisito dele, não estilo: ele decide se este processo continua vivo antes de
  qualquer outra inicialização. A segunda invocação agora **foca a janela da
  primeira**. `show()` antes de `unminimize()`, porque a janela pode estar recolhida
  na bandeja, e `unminimize` numa janela oculta não a torna visível.
- **A confirmação é condicional, e a condição é a decisão.** A ordem das cláusulas
  de `decidir_fechar` é o desenho: **recolher ganha de perguntar** (se nada se
  perde, confirmar é pedir aval para uma ação sem consequência — o caminho mais
  curto para ensinar alguém a clicar sem ler); **perguntar só com `anna` no ar**,
  porque aí fechar encerra a sessão **e** deixa processo órfão; **fechar calado no
  resto**, que é o que qualquer app faz.
- **Órfão não é hipótese:** havia dois `anna` vivos e um `shvia-desktop` zumbi de 14
  dias nesta máquina. Por isso, ao confirmar, o sidecar é morto **antes** de a
  janela ser destruída — depois do `destroy` o label sai do mapa e ninguém mais sabe
  qual `anna` era daquela janela.
- **`destroy()`, nunca `close()`:** `close` reemite `CloseRequested` e o guard
  perguntaria para sempre. E o diálogo roda em `std::thread::spawn` com
  `api.prevent_close()` chamado **antes** — `blocking_show` na main thread trava o
  app, que é a regra que o `updater::perguntar` já documenta nesta casa.
- **A decisão foi extraída para função pura** (`decidir_fechar`) para ser provável
  sem janela, sem bandeja e sem sidecar: **4 testes**, e a reversão foi conferida —
  invertendo a ordem das cláusulas, o primeiro reprova com
  `left: Perguntar, right: Recolher`.
- ⚠️ **Achado que fica aberto:** `desligar_recolher` é chamado quando a criação da
  bandeja **falha** e **persiste** `close_to_tray: false`. Mitigação de falha
  transitória que fica permanente — no Linux, onde bandeja falha por ambiente, isso
  desliga o item D2 para sempre sem ninguém saber. Não consertado aqui.

## 1.1.25 - O "Sobre" passa a mostrar o motor do Code, com a versão E a origem

- **O caso (19/08):** o Modo Code travava no 422 do gateway com o app na última
  versão e um `anna 0.8.4` de julho assado dentro dele. A versão do sidecar não
  aparecia em lugar nenhum — nem no app, nem na tela de erro, que mandava
  "atualize o app" enquanto o app já estava atualizado. Diagnóstico que depende
  de alguém rodar `--version` num binário escondido dentro de um bundle é
  diagnóstico que ninguém faz.
- **A linha "Motor do Code"** mostra o `anna` que ESTE app usaria, resolvido pelo
  mesmo `resolve_bin` que o Modo Code usa — então o que aparece é o que roda, não
  o que está no PATH de quem lê.
- **A origem vai junto** (`empacotado` / `externo`), e é ela que fecha o caso: o
  empacotado vence o do PATH por desenho, então instalar um `anna` novo no PATH
  não muda nada. Sem essa palavra na tela, a conclusão natural é a errada.
- **O "Copiar" carrega a mesma linha** — ele existe para colar em relato de
  suporte, e era exatamente essa a informação que faltava no relato.
- A saída do binário é **filtrada** antes de entrar na string JS do modal
  (alfanumérico, `.`, `-`, `+`, `_`, teto de 32): valor vindo de subprocesso não
  pode fechar aspa e virar código numa via de `eval`.

## 1.1.24 - O agente dentro do app volta a encontrar node, cargo e php: sidecar recebe o PATH do shell de login

- **App de GUI não herda PATH de shell.** No macOS o launchd entrega
  `/usr/bin:/bin:/usr/sbin:/sbin`, e o `anna` roda as ferramentas com `sh -c`
  herdando isso — então `npx`, `node`, `cargo`, `php` e `composer` (Homebrew,
  `~/.cargo/bin`, `~/.local/bin`) simplesmente não existiam dentro do app,
  embora funcionem no terminal.
- **O sintoma era o agente insistir, não avisar.** 19/08, projeto KIDS: 100
  voltas, 19 min, 125 ferramentas, US$ 1,24 repetindo `npx @gltf-transform/cli`
  impossível — com o `command not found` escondido atrás de `>/dev/null`. Subir
  o teto de voltas (anna 0.11.0) só deu mais corda; a causa era o ambiente.
- `src-tauri/src/user_env.rs`: PATH do `$SHELL -ilc` com marcadores contra
  banner de dotfile, watchdog de 3s, validação e união com o PATH atual (nunca
  substitui, preserva a ordem do usuário). Aplicado no `spawn` do
  `anna`/`claude-runner` e na sonda `command -v` do `resolve_bin` — que também
  não achava um `anna` instalado em `~/.local/bin`. Windows fica de fora
  (já recebe o PATH do usuário). Racional completo em ADR-030.
- Reempacota o **anna 0.11.1** (aviso do teto deixa de mostrar JSON de protocolo
  ao usuário — a string aparecia crua na timeline).

## 1.1.23 - Reempacota o anna 0.11.0: o teto de voltas do Modo Code deixa de matar turno legítimo na volta 25

- Rebuild sem mudança de código próprio: o estágio D5 empacota o `anna` do
  PATH na hora do build, e o `resolve_bin` do app prefere o binário bundlado —
  então a única forma de o Modo Code do desktop ganhar o teto novo é uma
  versão nova do app. A 1.1.22 (nunca publicada) carregava o anna 0.10.1, cujo
  teto fixo de 25 voltas matou um turno real de 26 ferramentas no projeto KIDS
  em 19/08 ("teto de voltas atingido — refine a pergunta").
- O anna 0.11.0 (SHVIA-CODE 1db563b) traz `max_voltas` configurável (env
  `SHVIA_MAX_VOLTAS` > config.toml > default 100) e, no host NDJSON, encerra o
  turno com a sessão viva — responder "continua" retoma de onde parou. No
  macOS o config.toml do anna fica em `~/Library/Application Support/shvia-code/`.

## 1.1.22 - O bump ganha prova: os seis portadores concordam, ou o build cai

- **A terceira ocorrência do bump pela metade.** `scripts/sync-version.mjs`
  sincronizava os portadores de versão a partir do `version.md` e **nunca
  falhava**: arquivo ausente e padrão de versão não encontrado eram `console.warn`
  \+ `continue`. Um portador que parasse de casar com o regex sairia da sincronia
  **para sempre**, com o build verde e ninguém sabendo.
- **O histórico que justifica:** 1.1.18→1.1.19 deixou os manifestos atrás; a
  1.1.19 existia justamente para consertar isso; e na 1.1.20 o `version.md` e o
  CHANGELOG foram na frente dos manifestos e **travaram a validação do `makepkg`**.
  Três vezes é padrão, não descuido.
- **O que muda:** o script agora **prova** o que fez — ao fim reconfere os cinco
  portadores contra o `version.md` e sai `exit 1` se algum discordar, nomeando
  quais. `npm run prova:bump` faz a mesma conferência **sem escrever**, para rodar
  antes de commitar: o build já sincronizava, e o que passava era a árvore
  **commitada**.
- **Reusa o mesmo par (regex, substituição) da sincronia** em vez de um segundo
  padrão para ler versão — segunda implementação é a que diverge calada.
- **Por que falhar e não avisar:** é a mesma regra do piso de versão do `anna`
  (1.1.21) e do `STANDING_ATIVO` nascer em `0`. Aviso em log de build é o que
  ninguém lê. Conferido por reversão nas duas classes de falha — portador em
  versão diferente e padrão que deixou de casar —, as duas saem `exit 1`.

## 1.1.21 - O empacotamento do `anna` ganha piso de versão: motor velho derruba o build em vez de virar release

- **O caso (19/08):** sessão longa do Modo Code travava no 422 do gateway
  (`messages` tem `max:200`). O `anna` 0.10.0 (18/08) compacta o histórico antes
  de enviar e resolve — mas o app saía com o `anna` que estivesse no PATH da
  máquina de build, e ali havia um **0.8.4 de julho**. Resultado: app na última
  versão, mensagem de erro mandando "atualize o app", e o app já atualizado. O
  que estava velho era o sidecar **dentro** dele.
- **O que muda:** `scripts/stage-anna.mjs` passa a exigir `ANNA_MINIMO` (0.10.0)
  e **derruba o build** abaixo disso, nomeando a versão achada e o comando de
  conserto. Ausência do `anna` continua não sendo erro — lá o Modo Code fica
  declaradamente indisponível; aqui ficaria disponível e quebrado, que é pior.
  É a mesma regra que o script já aplicava a binário que não responde
  `--version`, estendida a um caso a mais.
- **Por que não bastava o aviso:** o risco estava previsto no cabeçalho do
  próprio script desde o começo, com a mitigação *"imprime a versão para alguém
  conferir depois"*. O número estava no log do build; o log é que não tem leitor.
  Aviso perde para gesto — o mesmo argumento do `STANDING_ATIVO` nascer em `0`.

## 1.1.20 - Ícone do app entra na marca atual do ShvIA (o "S" azure) — o update parava de "trocar o ícone" porque o repo nunca trocou

- O usuário via o ícone antigo ("AI" + seta Blue3 sobre navy, design 0.3.1 de
  08/07) voltar a cada atualização e parecia bug do updater. NÃO era: o updater
  entrega exatamente o que o repo builda, e `src-tauri/icons/` + `brand/` nunca
  receberam a marca nova — o "S" azure existia só no site (favicon.svg do
  SHVIA-WEB, marca oficial de 25/07, quando o ShvIA ganhou identidade própria).
- Fonte nova `brand/shvia-desktop-icon-1024.png`: o favicon.svg oficial (bloco
  azure #34B3EC, rx≈23%, S em #0B0F17) embrulhado na grade de ícone do macOS
  (conteúdo 824×824 centrado em canvas 1024 transparente, margens 100px) e
  renderizado via qlmanage. Conjunto inteiro regenerado com `tauri icon`
  (icns/ico/png/Square*); os android/ e ios/ gerados foram descartados (mobile
  é outro repo). O tray de menu bar segue o template "AI" da 1.1.14 — decisão
  deliberada, não foi tocado.
- O ícone novo chega ao usuário na PRÓXIMA release publicada (o build embute o
  .icns). No Dock/Finder o macOS pode segurar cache de ícone da versão antiga;
  o app aberto e o alternador ⌘Tab mostram o novo imediatamente.

## 1.1.19 - O manifesto passa a mesclar por artefato, e o pacote do Arch para de sumir quando a Debian publica

O merge do `release.json` era **por plataforma**: `platforms[PLATAFORMA]` trocava
inteiro a cada build. Isso assumia "uma máquina por plataforma", e o Linux deixou de
caber nessa suposição na 1.1.16 — a máquina Arch gera o `.pkg.tar.zst` por `makepkg`
(ADR-028) e a Debian não gera nenhum. Publicar da Debian **apagava do manifesto** o
pacote pacman que a Arch tinha publicado.

É a mesma classe do bug que o download-antes-de-gerar do `--publish` já resolve entre
macOS/Windows/Linux, um nível abaixo — e pior de enxergar, porque as duas máquinas
escrevem na mesma chave `linux` e o manifesto resultante parece íntegro. O repositório
pacman em si sobrevivia (o `shvia.db` é arquivo separado); o que se perdia era a
entrada no manifesto, e com ela o `sha256` de quem baixa o pacote direto.

- **Mescla dentro da plataforma, chaveada pelo nome do arquivo.** O build atual sempre
  vence — artefato regerado substitui o hash antigo em vez de conviver com ele. Versão
  nova continua descartando o manifesto inteiro, então nada de outra release se acumula.
- **O log diz o que foi PRESERVADO de outra máquina.** Sem isso o operador veria "3
  artefatos" e um `release.json` com quatro, sem saber de onde veio o quarto — silêncio
  é o que fez este bug durar.
- **O aviso de "nenhum artefato assinado" ficou honesto.** Ele afirmava que o
  auto-update não ofereceria a versão; com a mescla isso pode ser falso, porque outra
  máquina já publicou artefato assinado da mesma release. A frase forte agora só sai
  quando o manifesto inteiro está sem assinatura.

Conferido com fixture das duas máquinas (Arch publica os 4 → Debian publica 3 → o
`.pkg` continua lá), com rehash e com troca de versão; e o comportamento antigo foi
reproduzido no script anterior para provar que o teste tem régua. Fecha a §2 das
pendências ativas do `.continue/estado-atual.md`.

## 1.1.18 - Conserta o update no Arch: o pacote convertido sai com o marcador, e o app deixa de depender só dele

O pacote pacman publicado na 1.1.17 (`shv-ia-1.1.17-1-x86_64.pkg.tar.zst`) foi
gerado por `fpm` numa máquina Debian, e portanto **sem** o marcador
`/usr/share/shvia-desktop/instalado-por`. No Arch, o app não sabia que tinha vindo
do pacman: pediu `?bundle=deb`, recebeu o `.deb`, chamou `pkexec dpkg -i` — que não
existe lá — e falhou no fim do download, exibindo um conselho sobre senha de
administrador que não tinha nada a ver com a causa. O ADR-028 previa esse cenário
por escrito e o build avisava no terminal; nada disso impediu o pacote de subir.

- **Guard novo no `updater.rs`, independente do empacotamento:** bundle se dizendo
  `deb` sem `dpkg` no sistema (ou `rpm` sem `rpm`) já é motivo para não oferecer o
  download. Ao contrário da leitura do marcador, aqui **não** há fail-open a
  preservar — não existe sistema onde essa instalação se atualize, então o download
  terminaria em erro de qualquer jeito. O marcador continua e tem precedência,
  porque ele permite a instrução exata (`sudo pacman -Syu`) em vez de um palpite.
  A busca do comando cobre o `PATH` e `/usr/sbin:/usr/bin:/sbin:/bin`, já que o
  plugin chama o instalador por `pkexec`/`sudo`, que montam PATH próprio.
- **A rota `fpm` do `build-local.sh` deixou de converter o `.deb` direto:** agora
  extrai o payload (`dpkg-deb -x`, ou `bsdtar` onde não houver), injeta o marcador e
  empacota com `-s dir`. Converter não deixava injetar arquivo nenhum — era a raiz
  do problema, não um detalhe de implementação.
- **Um nome só para o pacote: `shvia-desktop`.** O `fpm` vinha publicando `shv-ia`
  (nome herdado do pacote Debian) e o PKGBUILD, `shvia-desktop` — dois nomes para o
  mesmo app deixariam o `pacman -Syu` de quem instalou um cego para o outro, parado
  e sem erro na tela. `replaces`/`conflicts` nas duas rotas migram quem já instalou
  o `shv-ia`.
- **O `.pkg` de nome legado é removido do bundle dir antes de empacotar.** Ele tem a
  versão corrente no nome, então o `release-manifest.mjs` o aceitaria e o
  `find … | head -1` do `repo-add` poderia publicá-lo em vez do pacote novo.
- **ADR-029** conta o caso e revoga a consequência do ADR-028 que aceitava pacote sem
  marcador. `docs/build.md` perde o "conversão best-effort", e o `.continue/arch.md`
  — o molde para SSHVTERM-DESKTOP e GITHUB-DESKTOP — passa a exigir as duas camadas:
  quem copiar só o marcador herda esta falha.
- **Validado:** `cargo test` 48/48 (6 testes novos, incluindo o caso real desta versão
  e o contra-teste de que Debian/Fedora seguem atualizando), o novo conferido por
  reversão; `cargo clippy --all-targets` limpo; `bash -n`; e o bloco de empacotamento
  **executado** num sandbox com um `shv-ia-1.1.17` plantado (o `.deb` no disco ainda
  é o da 1.1.17) — saiu `shvia-desktop-1.1.17-1-x86_64.pkg.tar.zst` com o marcador dentro,
  `%REPLACES%`/`%CONFLICTS%` no `shvia.db` e o pacote legado apagado. Segue **não
  validado** o caminho `makepkg` (esta máquina é Debian 13) — pendência §1 do
  `.continue/estado-atual.md`, aberta desde a 1.1.16.

## 1.1.17 - Saneia o .continue: estado-atual descrevia a 0.8.0 com o repo na 1.1.16

- `estado-atual.md` reconstruído a partir do `git log` real. Ele listava como
  pendência coisa entregue há semanas — offline v2 (1.1.13), updater (1.0.0),
  tray (1.1.0), diagnóstico (1.1.4) e o `anna` no instalador (0.18.0) — e quem
  lesse ia refazer trabalho pronto. O que não deu para confirmar (microfone e
  Ctrl+V de imagem no WebKitGTK) ficou marcado como **não reavaliado**, não como
  pendente: afirmar que continua quebrado sem testar seria inventar status.
- Entram as duas pendências reais que a 1.1.16 criou: o caminho `makepkg` nunca
  rodou (foi escrito numa máquina Debian, sem makepkg/bsdtar/repo-add), e o
  `release-manifest.mjs` derruba a entrada do `.pkg` do manifesto quando se
  publica de uma máquina Linux que não gera pacote Arch.
- Dois links mortos consertados: `SAMIR-v1.md` (removido na 1.1.6) e a âncora
  `#decisões-em-aberto`, que no escopo é `#7-decisões-em-aberto`.
- `arch.md` encolhido para o que segue em aberto — SSHVTERM-DESKTOP e
  GITHUB-DESKTOP ainda na rota `fpm`. A parte que virou decisão estável está em
  docs/build.md e no ADR-028, que é a convenção da pasta (nota madura vira doc e
  sai daqui). Fica registrado ali que qualquer repo que ganhe pacote pacman
  herda o problema do auto-update, e sem o marcador entrega um update que falha
  no fim do download.
- `README.md` do `.continue` passa a listar o `arch.md`, que existia desde 04/08
  sem estar na tabela.

## 1.1.16 - Build nativo no Arch (makepkg + repo pacman) e o app para de tentar auto-update lá

- `build-local.sh` detecta a distro por `/etc/os-release` (`ID` + `ID_LIKE`, cobrindo
  Manjaro/EndeavourOS e Ubuntu/Mint). O preflight passa a sugerir `pacman -S` no Arch —
  antes mandava `apt-get install`, comando que não existe lá.
- Pacote Arch agora sai por `makepkg` com o novo `packaging/arch/PKGBUILD` quando o build
  roda num Arch; o `fpm` convertendo o `.deb` fica como fallback para máquina Debian.
  O PKGBUILD reempacota o `.deb` em vez de recompilar — mesmo binário, e sem levar a
  chave privada do updater para dentro do makepkg.
- `--publish` sobe também o banco do repositório (`repo-add`), então o usuário recebe
  versão nova com `pacman -Syu` em vez de baixar arquivo à mão.
- O app **não tenta mais se auto-atualizar quando foi instalado por pacman** (ADR-028):
  o `tauri-plugin-updater` não tem instalador de pacman e rodaria `pkexec dpkg -i` depois
  de baixar ~80 MB. O pacote instala `/usr/share/shvia-desktop/instalado-por` e o
  `updater.rs` lê o arquivo: havendo versão nova, avisa e manda rodar `pacman -Syu`.
- Revoga a nota da 1.1.15: o `.pkg.tar.zst` passa a entrar no `release.json` como artefato
  de download (sem `.sig`, e o endpoint ignora artefato sem assinatura — logo, inerte para
  o auto-update).
- **Não validado:** o caminho `makepkg` de ponta a ponta. A máquina onde isto foi escrito é
  Debian 13, sem `makepkg`/`bsdtar`/`repo-add` — precisa de uma passada num Arch.

## 1.1.15 - build-local.sh (Linux) ganha o pacote Arch (.pkg.tar.zst) via fpm

- O bundler do Tauri 2.x só gera deb/rpm/AppImage no Linux; o pacote pacman agora
  sai convertendo o .deb com o fpm na própria máquina Debian (sem host Arch).
  Deps mapeadas para os nomes do Arch (webkit2gtk-4.1, gtk3, libayatana-appindicator
  — o app usa tray-icon). Best-effort: sem fpm no PATH, avisa como instalar e segue.
- Fora do release.json/publish por enquanto: o fpm não gera o .sig do updater e o
  endpoint do ShvIA trata artefato sem assinatura como inexistente — entrar no
  manifesto quebraria a verificação da publicação. Distribuição manual por ora.
