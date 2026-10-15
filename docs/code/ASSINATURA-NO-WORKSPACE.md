# Connecting the Claude subscription to ShvIA from the desktop

> Since 1.29.0. The page half is ShvIA Web's Settings → Engines (`docs/API/CLAUDE-NO-WORKSPACE.md`
> there). This document is the desktop half: how `claude setup-token` is run and its token handed to
> the server without ever being in the page, and what was and was not proved.

## What the page can ask

Capability by presence: `recursos.assinatura` in the bridge — **true on Linux and macOS, false on
Windows** (the shell gives the client a pseudo-terminal; Windows has no equivalent in the app yet).
The three methods exist everywhere; the page asks the capability.

```text
claudeSubscriptionConnect({accountId?, accessToken}) → {fluxo} | {erro, codigo}
claudeSubscriptionCode({codigo})                     → {entregue} | {erro, codigo}
claudeSubscriptionCancel()                           → {cancelado}

events (onEvent):  {type:'claude_assinatura', fluxo, fase, url?, codigo?, mensagem?}
  fase  'aguardando'  the client started; url = the sign-in page, once the client showed one
        'enviando'    the person authorised; the token is on its way to the server
        'conectado'   the server answered that the subscription is connected
        'falhou'      codigo + mensagem (Portuguese fallback for a page that does not know the code)
        'cancelado'   the person (or the app quitting) stopped it
```

`accessToken` is the panel's token (`localStorage.access_token`), the one the page already uses to talk
to the server. `fluxo` is the number of the flow, so a page can tell a stale event from a current one.
Starting a flow cancels the one in progress.

Failure codes: `cancelado` (the person said no in the native dialog; nothing was opened) ·
`cli_ausente` · `spawn_falhou` · `pty_falhou` · `sessao_invalida` (the panel token is malformed or the
server answered 401) · `servidor_inseguro` · `servidor_inacessivel` · `servidor_ocupado` (429) ·
`servidor_com_erro` (5xx) · `servidor_recusou` (the server's own sentence in `mensagem`) ·
`resposta_inesperada` · `tempo_esgotado` (15 min) · `sem_token` · `codigo_invalido` · `sem_fluxo` ·
`nao_suportado` · the account codes of `claudeAccounts` (`conta_desconhecida`, `conta_indisponivel`).

## The flow

```text
page ── claudeSubscriptionConnect ──▶ shell   (off the UI thread)
                                       1. the server the app is configured for must be HTTPS (or this machine)
                                       2. NATIVE DIALOG: the profile and the server named; the page cannot answer it
                                       3. `claude setup-token` starts, on a pseudo-terminal of its own, with a lean
                                          environment and the profile's account variable
page ◀── {type:'claude_assinatura', fase:'aguardando', url?} ── the client opens the browser; the person authorises
                                       4. the client draws "✓ Long-lived authentication token created
                                          successfully! Your OAuth token (valid for 1 year): sk-ant-oat01-…"
                                       5. the shell reads the token off that screen
page ◀── 'enviando'                    6. PUT <configured server>/api/auth/me/claude-subscription
page ◀── 'conectado'                      with the panel's token; the server stores it encrypted
```

The token goes from step 5 to step 6 inside the shell. It is not in the page, in an event, in a log, on
a command line or in an environment variable.

## What was read, and what was NOT run

The installed client (2.1.300) was **read**, not run: its `setup-token` action renders an Ink screen
(`ConsoleOAuthFlow`, mode `setup-token`), starts the browser sign-in at once, shows the sign-in URL
("Browser didn't open? Use the url below to sign in") and a paste prompt for a code, and on success
draws the token once, between the lines "Your OAuth token (valid for 1 year):" and "Store this token
securely. You won't be able to see it again.", then leaves with code 0. A hidden `expiresInDays`
option defaults to 365. A policy of the machine (`forceLoginMethod`, `allowedProviders`) can refuse the
command; its message ends up as `sem_token`.

The sign-in needs the person's browser, so **no test here can run it**. What is proved:

- the screen reader, on fixtures shaped like that screen (colour and cursor codes, an OSC 8 hyperlink,
  a token the terminal wrapped in two, the same frame drawn twice, an API key where a subscription
  token should be, a token still arriving);
- the process handling, on stand-ins that draw the screen: a real terminal and controlling terminal,
  the lean environment, the whole flow with its events, a timeout and a cancel that end the client **and
  what it started**, a client that ignores TERM (it gets KILL), the code typed into the terminal;
- the delivery, against a local server that speaks HTTP: the request is the one Settings → Engines makes,
  a 401/422/429/5xx/redirect each become their code, a network failure is retried, a "no" is not.

What is not proved is the real client's output on the owner's machine. The runbook below is that proof.

## Why a pseudo-terminal

`claude auth login` was driven with pipes (`login.rs`: it has a plain prompt). `setup-token` is an Ink
screen: it wants a terminal for the keyboard input of its paste prompt, and draws with escape codes.
The shell opens one (`openpty`), makes it the client's controlling terminal (`setsid` + `TIOCSCTTY`),
and makes it 1000 columns wide: the client wraps a long line at the terminal's width, and a token cut by
a wrap is the one thing that must never be handed on. If a wrap happens anyway, the lines between the
client's own labels are joined; without the labels, a run that a wrap may have cut is refused.

Ending a flow is a signal to the client's **process group** (it called `setsid`, so the group is its
own), by id and never by name: whatever the client started goes with it. TERM first, KILL after a
grace.

## Why the token does not go through the page

1. **Nothing that runs in the server's origin can read it back.** The page is first-party, but it is the
   surface a script injected into the origin would run in; the secret that opens a subscription is not
   worth that exposure.
2. **The flow finishes without the page.** A person who goes elsewhere in the app while the browser is
   open still ends up connected.
3. **The destination is the configured server**, never an address the page names; HTTPS (or this
   machine) only, no redirect followed.

The panel's token the page hands over is already the page's own to hand; the shell uses it for one
request and keeps it only for the flow's life.

## Hygiene

- **Native dialog first**, naming the profile and the server (same rule as the login: the page cannot
  answer it).
- **Lean environment:** the client gets `HOME`, a locale, `XDG_*`, the desktop and D-Bus variables it
  opens the browser with, the proxy and certificate variables, `PATH` (the person's real one) and
  `TERM`. A key the app was launched with (`ANTHROPIC_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`, a cloud
  credential, a ShvIA token) is not passed, so the client behaves the same whatever the shell that
  started the app had exported.
- **The account:** the profile the page names (an id from the closed list, never a path) sets the
  account variable, as everywhere else. `setup-token` stores nothing on disk; **the identity that is
  connected is whichever account the person authorises in the browser**, not the profile's.
- **`Segredo`:** the token's type has no `Display`, a `Debug` that prints a mark, no `Clone`, and
  overwrites its bytes on drop. The overwrite is best effort (the HTTP client keeps its own copy of the
  body); the rule that matters is that a `{:?}` in a message cannot leak it.
- **A closed set of fields** reaches the page (`para_json`): number, phase, URL, code, message.
  Nothing read off the screen except the sign-in URL, and that only from Claude's own hosts.
- **A ruler reads the source:** nothing in the module prints or logs
  (`nada_neste_modulo_imprime_ou_registra`).

## The token's own limits

The client's source says tokens from `setup-token` are limited to **inference** (they cannot use Remote
Control and similar). They last a year. Disconnecting in Settings → Engines forgets the copy ShvIA
holds; the token itself stays valid at Anthropic until it expires, and can be revoked from the Claude
account.

## Owner's runbook (the proof that needs a browser)

On Linux or macOS, in the desktop app, signed in to ShvIA, with `claude` installed:

1. Settings → Engines. A **Connect from the app** button is there (in a plain browser it is not).
2. Click it. A native dialog names the profile and the server. Answer **Cancel**: nothing opens, the
   page says it was cancelled. (Check: `ps aux | grep 'claude setup-token'` finds nothing.)
3. Click it again and answer **Connect**. The browser opens on Claude's sign-in page. Authorise the
   account you want connected.
4. The page goes through "waiting" → "sending" → "connected · ending in … · valid until …". The token
   appeared nowhere on the screen.
5. Reload Settings → Engines: the same status is read from the server. (`ps aux | grep claude` finds no
   `setup-token` left.)
6. Click **Connect** again and **Cancel** in the page while the browser is open: the page says
   cancelled, and the `claude` process is gone.
7. If step 3 opens no browser, the page shows a link and a field: open the link, copy the code the
   browser shows, paste it in the field.

If step 4 ends in "the client finished without a token I recognise", the screen's wording changed:
note the client version (`claude --version`), and use the paste path meanwhile — it still works.

## Tests

`cargo test --lib assinatura` — the screen reader (`tela.rs`), the terminal and the flow (`pty.rs`), the
delivery (`servidor.rs`), and the module's source rulers (`assinatura.rs`). The bridge-wide ruler
`bracos_lentos_saem_da_thread_da_interface` includes both blocking arms, and the Windows target is
checked with `cargo clippy --target x86_64-pc-windows-gnu`.
