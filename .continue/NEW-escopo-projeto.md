# Provedores de Nuvem no Gateway de Inferência — Especificação

> Documento de implantação para o Claude Code.
> Stack: Laravel 13 / PHP 8.3+ / MariaDB / Apache2 + php-fpm / Debian Trixie.
> Foco: adicionar **provedores de LLM em nuvem** (Anthropic, OpenAI, Z.ai, xAI/Grok)
> ao gateway único de inferência que já existe, **sem** criar endpoints paralelos,
> reaproveitando `POST /api/v1/chat`, `ModelCatalog`, `inference_requests` e o
> HealthProbe. Introduz uma camada fina de **drivers** para lidar com os três
> "sotaques" de API (Ollama nativo, OpenAI-compat, Anthropic Messages) sem
> espalhar `if (provider == …)` pelo controller.
>
> **Data:** 2026-07-01 · **Status:** aberto, aguardando decisões D1–D9.

---

## 1. TL;DR

- **Não** criar rota nova. O ponto de entrada continua sendo `POST /api/v1/chat`
  com SSE. Provedor de nuvem é só mais um `server` no mesmo gateway, do lado dos
  Ollama.
- **Um driver por sotaque de API**, não um por fornecedor:
  - `ollama` — o nativo que já existe (discovery via `/api/tags`, stream NDJSON).
  - `openai` — **cobre OpenAI, xAI/Grok e Z.ai** (os três são OpenAI-compat:
    `/chat/completions`, `Authorization: Bearer`, bloco `usage`).
  - `anthropic` — a exceção (`/v1/messages`, header `x-api-key` + `anthropic-version`,
    SSE de eventos tipados, `system` fora da lista de mensagens).
- **Generalizar** `config/ollama.php` (par fixo local/remote) para um mapa de N
  servers, cada um com uma chave `driver` e um campo novo `data_locality`.
- **Whitelist curada** de modelos por provedor de nuvem (não descoberta ao vivo):
  nomes de modelo rotacionam rápido e cada chamada custa dinheiro.
- **Guarda-corpo LGPD** obrigatório: server com `data_locality != on_prem`
  **não recebe** o contexto estruturado de cliente (CPF/contrato/PPPoE/telefone
  do CONTEXTO-CLIENTE). Z.ai (servidores na China) entra como sensibilidade máxima.
- Auditoria (`inference_requests`) ganha uma coluna `data_locality` pra registrar
  quando o dado saiu de casa. Zero chave de API em log.

---

## 2. O que já existe (não duplicar)

A implementação **estende**, não recria. Peças centrais já no repo:

| Peça | Onde (confirmar nome real ao ler o código) | Ação |
| ---- | ---- | ---- |
| Endpoint único de chat + SSE | `App\Http\Controllers\Api\ChatController` (`POST /api/v1/chat`) | Consumir do driver em vez de falar direto com Ollama |
| Resolução perfil → servidor | `App\Services\AI\ModelCatalog::resolveForChat` | Generalizar p/ qualquer `server_key` + resolver o driver |
| Config de servidores | `config/ollama.php` (`servers.local`, `servers.remote`) | Virar mapa de N servers com `driver` + `data_locality` |
| Catálogo de modelos | `App\Services\AI\ModelCatalog` (lê `/api/tags` c/ cache) | Ollama = discovery; nuvem = whitelist da config |
| Health por servidor | `App\Services\HealthProbe` + `/api/v1/health` | Já é N servers; adicionar probe barato p/ nuvem |
| Auditoria de inferência | tabela `inference_requests` + canal de log `inference` | Adicionar coluna `data_locality`; tokens vêm do `usage` do provedor |
| Tokens por chamada | `prompt_tokens` / `completion_tokens` / `total_tokens` (TOKEN-TRACKING) | Mapear do formato de cada provedor |
| Modelo gravado na msg | `messages.model_name` + `messages.metadata.server` | Gravar o `server_key` de nuvem |
| IDs compostos `model@server` | especificado em API-OPENAI-COMPAT-20260616 | Reusar a mesma convenção (ver D4) |
| Whitelist de modelos | MODELS-WHITELIST | Reaproveitar o conceito p/ os modelos de nuvem |

**Regra:** se a peça acima já cobre o caso, estender. Se é domínio novo (a camada
de drivers), criar em `App\Services\AI\Drivers\`.

---

## 3. Desenho

### 3.1 A camada de drivers

Uma interface e três implementações. O controller passa a ser **agnóstico de
provedor**: ele pede ao driver que faça o stream e recebe eventos já no formato
interno do SHVIA (o mesmo SSE que ele emite hoje). Toda a tradução de sotaque
fica dentro do driver.

```php
namespace App\Services\AI\Drivers;

interface ChatDriver
{
    /** Modelos expostos por este server (whitelist da config, ou /api/tags no Ollama). */
    public function listModels(array $server): array;

    /**
     * Roda o chat e emite eventos normalizados via callback $onEvent.
     * Contrato do evento interno (o que o ChatController já sabe emitir):
     *   ['type' => 'delta', 'text' => '…']
     *   ['type' => 'usage', 'prompt_tokens' => int, 'completion_tokens' => int, 'total_tokens' => int]
     *   ['type' => 'done']
     *   ['type' => 'error', 'message' => '…']
     */
    public function streamChat(array $server, array $messages, array $options, callable $onEvent): void;

    /** Health barato — não gasta quota de chat. */
    public function health(array $server): array;
}
```

Implementações:

- **`OllamaDriver`** — refatoração do que já existe hoje no `ModelCatalog`/`ChatController`.
  Discovery `/api/tags`; stream NDJSON linha-a-linha (`message.content` = delta;
  na linha final, `prompt_eval_count`/`eval_count` → prompt/completion tokens).
- **`OpenAiCompatDriver`** — serve **OpenAI, xAI/Grok e Z.ai**. `POST {base_url}/chat/completions`,
  `stream: true`, `Authorization: Bearer {api_key}`. SSE: cada `data:` traz
  `choices[0].delta.content`; sentinela `data: [DONE]`; `usage` no chunk final
  (na OpenAI exige `stream_options: {"include_usage": true}` p/ vir usage no stream).
  Quirks tratados por config (ver 3.3), não por `if` de fornecedor.
- **`AnthropicDriver`** — `POST {base_url}/messages`, headers `x-api-key` +
  `anthropic-version`. SSE tipado: `content_block_delta.delta.text` = delta;
  `message_start.usage.input_tokens` e `message_delta.usage.output_tokens` = tokens.
  **Atenção ao formato de entrada**: `system` é parâmetro top-level (não é mensagem),
  e as mensagens alternam `user`/`assistant`. O adaptador converte o formato
  interno do SHVIA p/ esse shape.

Um **registry/factory** resolve `server_key → driver` lendo `config('...servers.<key>.driver')`.

### 3.2 Config generalizada

`config/ollama.php` (ou `config/providers.php`, ver D1) vira um mapa. Exemplo
ilustrativo — os nomes de modelo são chute de partida, **confirmar em D7**:

```php
return [
    'default_server' => env('AI_DEFAULT_SERVER', 'shvia1'),

    'servers' => [

        // ── Ollama on-prem (driver nativo, discovery via /api/tags) ──
        'shvia1' => [
            'driver'        => 'ollama',
            'base_url'      => env('OLLAMA_HOST_SHVIA1', 'http://10.x.x.x:11434'),
            'timeout'       => 300,
            'enabled'       => true,
            'data_locality' => 'on_prem',   // dado não sai da Blue3
            'models'        => null,         // null = descobre via /api/tags
        ],
        'samirb3' => [
            'driver'        => 'ollama',
            'base_url'      => env('OLLAMA_HOST_SAMIRB3', 'http://10.x.x.x:11434'),
            'timeout'       => 300,
            'enabled'       => true,
            'data_locality' => 'on_prem',
            'models'        => null,
        ],

        // ── OpenAI-compat: OpenAI, xAI/Grok, Z.ai (MESMO driver) ──
        'openai' => [
            'driver'        => 'openai',
            'base_url'      => 'https://api.openai.com/v1',
            'api_key'       => env('OPENAI_API_KEY'),
            'timeout'       => 120,
            'enabled'       => (bool) env('OPENAI_API_KEY'),
            'data_locality' => 'cloud_us',
            'models'        => [
                ['id' => 'gpt-4o',  'label' => 'GPT-4o'],
                ['id' => 'o4-mini', 'label' => 'o4-mini'],
            ],
        ],
        'xai' => [
            'driver'        => 'openai',
            // Região UE existe: https://eu-west-1.api.x.ai/v1 (ver LGPD, §4)
            'base_url'      => env('XAI_BASE_URL', 'https://api.x.ai/v1'),
            'api_key'       => env('XAI_API_KEY'),
            'timeout'       => 300,          // modelo de reasoning demora
            'enabled'       => (bool) env('XAI_API_KEY'),
            'data_locality' => 'cloud_us',
            // Quirk Grok reasoning: usa max_completion_tokens, não max_tokens
            'quirks'        => ['max_tokens_field' => 'max_completion_tokens'],
            'models'        => [
                ['id' => 'grok-4.3', 'label' => 'Grok 4.3'],
            ],
        ],
        'zai' => [
            'driver'        => 'openai',
            // ⚠️ a base JÁ termina em /v4 — NÃO acrescentar /v1 (senão 404)
            'base_url'      => env('ZAI_BASE_URL', 'https://api.z.ai/api/paas/v4'),
            'api_key'       => env('ZAI_API_KEY'),
            'timeout'       => 300,
            'enabled'       => (bool) env('ZAI_API_KEY'),
            'data_locality' => 'cloud_cn',   // servidores na China — sensibilidade MÁXIMA
            'models'        => [
                ['id' => 'glm-4.6', 'label' => 'GLM-4.6'],
                ['id' => 'glm-5.1', 'label' => 'GLM-5.1'],
            ],
        ],

        // ── Anthropic (driver próprio: /v1/messages) ──
        'anthropic' => [
            'driver'            => 'anthropic',
            'base_url'          => 'https://api.anthropic.com/v1',
            'api_key'           => env('ANTHROPIC_API_KEY'),
            'anthropic_version' => '2023-06-01',
            'timeout'           => 300,
            'enabled'           => (bool) env('ANTHROPIC_API_KEY'),
            'data_locality'     => 'cloud_us',
            'models'            => [
                ['id' => 'claude-opus-4-8',   'label' => 'Claude Opus 4.8'],
                ['id' => 'claude-sonnet-4-6', 'label' => 'Claude Sonnet 4.6'],
            ],
        ],
    ],
];
```

Chaves ficam **só no `.env`** (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `XAI_API_KEY`,
`ZAI_API_KEY`) e **nunca** entram em log nem em `inference_requests`.

### 3.3 Endpoints e sotaques (verificado 2026-07-01 — ver §7, rotacionam)

| Server | `driver` | Base URL | Auth | Stream | Nota |
|---|---|---|---|---|---|
| OpenAI | `openai` | `https://api.openai.com/v1` | Bearer | SSE `data:` + `[DONE]` | `stream_options.include_usage=true` p/ usage |
| xAI/Grok | `openai` | `https://api.x.ai/v1` | Bearer | SSE `data:` + `[DONE]` | reasoning usa `max_completion_tokens`; região UE disponível |
| Z.ai | `openai` | `https://api.z.ai/api/paas/v4` | Bearer | SSE `data:` + `[DONE]` | **base termina em `/v4`, não pôr `/v1`**; servidores na China |
| Anthropic | `anthropic` | `https://api.anthropic.com/v1` | `x-api-key` | SSE eventos tipados | `system` top-level; `anthropic-version` header |

---

## 4. LGPD / residência de dados (parte crítica, não opcional)

Adicionar nuvem significa que **o prompt sai da Blue3**. Isso colide de frente com
o CONTEXTO-CLIENTE (Modo A: agente busca por contrato/CPF/PPPoE/telefone e o dado
do cliente é injetado no prompt). Regras:

1. **Bloqueio de injeção em nuvem.** Todo server tem `data_locality`
   (`on_prem` | `cloud_us` | `cloud_cn`). A camada que injeta contexto de cliente
   **deve pular / recusar** a injeção quando o server resolvido não é `on_prem`.
   Contexto de cliente (PII) só viaja para Ollama local. Força D5.
2. **Z.ai = sensibilidade máxima.** Servidores na China, sem adequação LGPD.
   Recomendo Z.ai atrás de allowlist explícita de admin e **nunca** em nenhum
   fluxo que toque PII de cliente — nem com bloqueio de injeção, por precaução
   jurisdicional.
3. **Auditoria registra a saída.** `inference_requests` ganha `data_locality`
   (e opcionalmente um booleano `client_context_injected`, que tem que ser
   sempre `false` quando `data_locality != on_prem`). Isso deixa provável, em
   auditoria, que dado nenhum de cliente saiu de casa.
4. **Sinalização na UI.** Modelo de nuvem aparece com selo "dados saem da Blue3"
   no seletor, pro agente saber o que está fazendo (ver D-badge em D9).
5. **Gate de acesso.** Modelo de nuvem custa dinheiro real por token — restringir
   por `UserProfile`/work group (amarra no sistema de quota que já existe).

---

## 5. Pontos de decisão (D1–D9)

- **D1 — Arquivo de config.** Generalizar `config/ollama.php` no lugar (nome passa
  a mentir) **ou** criar `config/providers.php` e deixar `ollama.php` como shim/deprecado?
  *Recomendo `providers.php`* — o domínio cresceu além de Ollama. Tua chamada.
- **D2 — Granularidade do driver.** Um `OpenAiCompatDriver` compartilhado (OpenAI+xAI+Z.ai)
  vs. um driver por fornecedor. *Recomendo compartilhado*, quirks por config.
- **D3 — Exposição de modelos.** Whitelist na config vs. discovery ao vivo (`/v1/models`).
  *Recomendo whitelist curada.* Opcional: comando `php artisan providers:models {server}`
  só p/ **inspecionar** o catálogo atual e te ajudar a curar (já que rotacionam).
- **D4 — Convenção de ID.** Reusar `model@server` do API-OPENAI-COMPAT
  (`claude-opus-4-8@anthropic`, `grok-4.3@xai`, `glm-4.6@zai`, `ShvIA:G4v5@shvia1`)?
  *Recomendo sim* — consistência com o facade que você já especou.
- **D5 — Força do guarda-corpo LGPD.** Bloqueio duro (contexto de cliente + nuvem = recusa)
  vs. bloqueio suave (pula injeção, permite chat genérico) vs. opt-in com log de consentimento.
  *Recomendo: pular injeção sempre + bloquear Z.ai em qualquer caminho com PII.*
- **D6 — Custo.** Só tokens por enquanto (recomendo) vs. mapa de preços + coluna `cost_usd`
  já agora. Dá pra derivar custo depois de um mapa estático; não construir billing agora.
- **D7 — Modelos-semente por provedor.** Os nomes do §3.2 são partida. **Confirmar**
  contra `/v1/models` de cada um (o Grok redireciona slugs antigos p/ `grok-4.3`;
  Z.ai lançou GLM-5.x). Quais modelos entram no ar no dia 1?
- **D8 — Falha/timeout/429.** Fail-fast + evento SSE `error` claro (recomendo) vs.
  retry/fallback p/ outro server. Nuvem tem rate limit e latência diferentes.
- **D9 — Gate de acesso + selo.** Quais `UserProfile`/work groups podem usar modelo
  pago de nuvem, e o modelo de nuvem leva selo "off-prem" no dropdown?
  *Recomendo: admin + grupo opt-in explícito no início, amarrado à quota.*

---

## 6. Critérios de aceite (verificáveis)

- [ ] `config/providers.php` (ou `ollama.php` generalizado) carrega N servers com
      drivers mistos; `php artisan config:cache` sem erro.
- [ ] `POST /api/v1/chat` faz stream **idêntico** do ponto de vista do cliente
      (SSE inalterado) para **um modelo de cada driver**: um Ollama (Anna), um
      OpenAI-compat (Grok ou GPT) e um Anthropic (Opus). Provado por `curl` mostrando
      o stream de tokens + o evento `usage` final.
- [ ] `messages.model_name` + `messages.metadata.server` gravam o `server_key` de nuvem.
- [ ] Linha em `inference_requests` escrita no `finally` de cada chamada de nuvem,
      com `server_key`, modelo, tokens (do `usage` do provedor), duração e
      `data_locality`. **Nenhuma** chave de API em log algum.
- [ ] `/api/v1/health` reporta cada server de nuvem (up / auth-error / down) via
      probe barato (lista de modelos), **sem** consumir quota de chat.
- [ ] **Teste LGPD:** requisição roteada p/ server de nuvem com contexto de cliente
      solicitado **não** envia o PII no payload de saída (injeção pulada/bloqueada).
      Provar logando o corpo de saída num teste de dev com um CPF fake e confirmando
      que ele nunca aparece na requisição ao provedor.
- [ ] Seletor de modelo lista os de nuvem agrupados por provedor, com selo off-prem
      (se D9 aprovar o selo).
- [ ] **Rollback:** reverter config + drivers volta o caminho Ollama-only intacto.

---

## 7. Referências (verificado 2026-07-01 — nomes de modelo rotacionam, reconferir)

- OpenAI — `https://api.openai.com/v1`, `/chat/completions`, Bearer.
- Anthropic — `https://docs.claude.com/en/api/overview` · `/v1/messages`, `x-api-key`, `anthropic-version`.
- xAI/Grok — `https://docs.x.ai` · base `https://api.x.ai/v1` (região UE `https://eu-west-1.api.x.ai/v1`); flagship atual `grok-4.3`; slugs antigos redirecionados desde 15/05/2026.
- Z.ai (Zhipu) — `https://docs.z.ai` · base OpenAI-compat `https://api.z.ai/api/paas/v4` (**não** anexar `/v1`); também expõe endpoint Anthropic-compat em `/api/anthropic`; servidores na China.

---

## 8. Prompt de handoff para o Claude Code

> Colar no Claude Code. **Ele deve ler os arquivos reais e devolver os nomes/assinaturas
> de verdade ANTES de escrever qualquer código. Sem commit sem revisão do Samir.**

````markdown
# Tarefa: adicionar provedores de nuvem (Anthropic, OpenAI, Z.ai, xAI) ao gateway

## Fase 0 — Ler e RELATAR (não escrever código ainda)
Leia e me devolva, em texto, o estado real de:
1. `config/ollama.php` — estrutura atual de `servers` (chaves, campos).
2. `app/Services/AI/ModelCatalog.php` — assinatura de `resolveForChat`, como
   descobre modelos (`/api/tags`) e como hoje escolhe local vs remote.
3. `app/Http/Controllers/Api/ChatController.php` — o loop de SSE: quais eventos
   ele emite hoje (nomes/campos exatos), onde fala com o Ollama, e o que tem no
   bloco `finally` que grava `inference_requests`.
4. `app/Services/HealthProbe.php` — como monta `services.ollama.<key>` no `/api/v1/health`.
5. A migration + model de `inference_requests` — colunas atuais.
6. Onde o **contexto de cliente** (CONTEXTO-CLIENTE, Modo A) é injetado no prompt —
   arquivo, método e ponto exato de injeção.

Pare aqui e me mostre esse relatório + o desenho proposto (interface `ChatDriver`,
onde colocam os drivers, shape final da config). **Espere meu OK.**

## Fase 1 — Implementar (só após meu OK)
- Criar `App\Services\AI\Drivers\ChatDriver` + `OllamaDriver` (refatorar o atual),
  `OpenAiCompatDriver` (OpenAI/xAI/Z.ai), `AnthropicDriver`, e um factory por `server_key`.
- Generalizar a config p/ mapa de N servers com `driver` + `data_locality` (D1).
- `ChatController` passa a consumir do driver (agnóstico de provedor); manter o
  SSE e o `finally` de auditoria idênticos.
- Adicionar coluna `data_locality` (e `client_context_injected`) em `inference_requests`.
- Guarda-corpo LGPD: pular/bloquear injeção de contexto de cliente quando
  `data_locality != on_prem` (D5). Z.ai fora de qualquer caminho com PII.
- HealthProbe: probe barato de nuvem (lista de modelos, sem gastar quota).
- Diff mínimo. Não tocar em nada fora do escopo. Sem commit — me mostra o diff.

## Deploy (produção, quando aprovado)
`git fetch` + `git reset --hard origin/master` (não `git pull`).
````

---

**Fim.** Decisões que travam o início: D1 (arquivo de config), D5 (força do
guarda-corpo LGPD) e D7 (quais modelos no dia 1). O resto dá p/ decidir na revisão do diff.
