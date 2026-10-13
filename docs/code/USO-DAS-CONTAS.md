# Account usage — the plan limits behind the Usage tab

> Since 1.27.0. The page side is ShvIA Web's Usage tab in the Code panel (`docs/FRONTEND/ABA-USO.md`
> there). This document is the desktop half: how an account's plan limits are read, and why that way.

## What the page can ask

Capability by presence: `recursos.uso` in the bridge, and the two methods.

```text
claudeUsage({accountId?}) → {type:'uso', plano, janelas, semJanelas?} | {erro, codigo}
codexUsage()              → {type:'uso', plano, janelas}              | {erro, codigo}

janelas: [{ chave, nome, minutos, usado, renova }]
  chave   'cinco_horas' | 'semanal' | 'semanal_modelo' | 'credito_extra' | 'janela'
  nome    the server's own label for a per-model window ("Fable"), else null
  minutos a 'janela' of unusual length, else null
  usado   PERCENT USED, 0–100, or null when the server had no number
  renova  ISO 8601, or null
codigo: 'uso_indisponivel' | 'runner_sem_uso' | 'uso_falhou' | the engine-absent codes
```

The number is always **percent used**. Both sources already say "used" (`utilization`,
`usedPercent`), so nothing is inverted anywhere; a page that shows "left" would be choosing to.

## Where the numbers come from

**Claude.** `claude-runner --uso` opens a prompt-less query and calls the Agent SDK's `/usage`
reader with `skipBehaviors` — a control request, no turn and no tokens — under the account's profile
variables, exactly as `claudeModels` and `spawn` do. The Claude Code client reads the limits with the
credential it keeps; the desktop never handles a token. `claude-runner/uso.mjs` normalises
`five_hour`, `seven_day`, `model_scoped[]` and, when on, `extra_usage`; the answer's code-named
buckets and its `limits[]` array are undocumented and never reach the page.

The SDK names the reader `usage_EXPERIMENTAL_MAY_CHANGE_DO_NOT_RELY_ON_THIS_API_YET` (0.3.287) and
says the name will change. The runner finds it by presence (`getUsage`, `usage`, then any `usage_*`
method); without one it answers `uso_indisponivel` and the page says so.

**Codex.** `codex-runner --uso` asks the app-server `account/rateLimits/read` (no params: the vendored
schema predates them). `codex-runner/uso.mjs` names each window by its **length**, never by its
slot — measured on a `prolite` plan, `primary` was the weekly window and `secondary` was null.

## The usage home of a credential-only profile

🔴 The Claude Code client caches the limits in its configuration home (`cachedUsageUtilization` in
`.claude.json`). A profile on `CLAUDE_SECURESTORAGE_CONFIG_DIR` moves only the credential key, so
every such profile shares that home. Measured on 13/10/2026 with `claude-me` and `claude-b3`: read
one after the other, the second answered with the **first one's numbers**, to the microsecond, while
its own plan (`team` against `max`) came out right.

So the read of such a profile also gets `CLAUDE_CONFIG_DIR`, pointing at a home of its own in the
app's cache — `<app cache>/uso-claude/<account id>` (`contas_claude::casa_para_uso`). Never the
profile's directory: a first trial pointed it there and the client wrote a full configuration home
(`.claude.json`, `projects/`, `sessions/`) into the person's profile folder. With the home in the
app's cache, the two accounts read their own numbers and the profile folders are untouched. A
`CLAUDE_CONFIG_DIR` profile already has its own home, and the default account has no other; neither
gets one.

Known limit: the default account still shares `~/.claude` with every credential-only profile the
person runs by hand, so its cached answer can be another account's. The page labels each reading
with the account it asked for; it cannot see a cache it does not own.

## Tests

- `claude-runner/uso.test.mjs`, `codex-runner/uso.test.mjs` — the normalisers, on the real answers of
  13/10/2026 (`npm run prova:politica`).
- `contas_claude::tests::a_leitura_de_uso_so_isola_o_perfil_que_divide_a_casa` and
  `motores::tests_motor::a_resposta_de_uso_tem_uma_forma_so` (`cargo test`).
- `scripts/prova-instalador-copia-os-imports.mjs` now also reads `import("./x.mjs")`: the runner loads
  `uso.mjs` dynamically, and the static-only reading would have let it out of both installers.
