// Casca de bootstrap do ShvIA Desktop.
//
// A janela Tauri abre esta casca local (instantânea, sem rede) com um splash da
// marca e navega para o ShvIA hospedado — a partir daí a UI é o próprio Blade
// do ShvIA (ADR-002; docs/arquitetura.md). Backport da casca do SHVIA-MOBILE
// (mantê-las em paridade): em vez de navegar às cegas (que estampa a tela de
// erro nativa do WebView quando rede/VPN está fora), a casca VERIFICA o
// servidor antes:
//   1. probe de alcance NO RUST (`shvia_server_probe`);
//   2. alcançável → location.replace() (o splash não fica no histórico);
//   3. falhou → estado "offline" com "Tentar agora" (e "Abrir mesmo assim"
//      como escape, caso o probe falhe por outra razão que não a rede).
//
// Offline com AUTO-RETRY: além do botão, a casca fica tentando sozinha a cada
// 5 s e no evento `online` do navegador — voltou a rede, entra sem clique
// (pedido do Samir, 07/07).
//
// ── O que mudou no item D4 (ADR-019) ──────────────────────────────────────────
// O endereço do servidor NÃO é mais constante aqui: vem do Rust
// (`shvia_server_config`), que lê a configuração do dono da máquina e cai no
// embutido quando não há nada. É o que fecha o item (a) da F2 e habilita on-prem
// e apontar para um servidor local em desenvolvimento.
//
// E o probe **saiu do JavaScript**, por duas razões:
//   - o `csp` do tauri.conf.json é ESTÁTICO, e `connect-src` estático não pode
//     listar uma URL que o usuário acabou de digitar. Com o `fetch` aqui,
//     servidor configurável era impossível — não por decisão, por CSP;
//   - o primeiro request de rede do WebKit "frio" custava 5-6 s antes de
//     qualquer resposta (ADR-012), o que forçava um timeout de 15 s e fazia o
//     app parecer travado no splash. O `connect` no Rust responde em dezenas de
//     milissegundos.
//
// Dev: "?hold" congela no estado connecting e "?hold=offline" mostra o estado
// offline — pra estilizar o splash sem ser redirecionado (sem auto-retry).
// "?server" abre direto o formulário de servidor.

import { invoke } from "@tauri-apps/api/core";

/** Espelha `server::ServerConfig` do Rust. */
type ServerConfig = {
  url: string;
  is_default: boolean;
  default_url: string;
};

// Fallback de última instância: só vale se o `invoke` falhar (casca aberta fora
// do Tauri, ex.: `vite` no navegador para estilizar). A fonte da verdade é o
// Rust — `server::DEFAULT_URL`.
const FALLBACK_URL = "https://ai.shvia.org";
// Intervalo do auto-retry no estado offline.
const AUTO_RETRY_MS = 5_000;

const rootEl = document.getElementById("bootstrap")!;
const statusEl = document.getElementById("status")!;
const retryEl = document.getElementById("retry");
const forceEl = document.getElementById("force-open");
const serverLabelEl = document.getElementById("server-label")!;
const changeEl = document.getElementById("server-change");
const formEl = document.getElementById("server-form") as HTMLFormElement | null;
const inputEl = document.getElementById("server-url") as HTMLInputElement | null;
const errorEl = document.getElementById("server-error")!;
const resetEl = document.getElementById("server-reset") as HTMLButtonElement | null;
const cancelEl = document.getElementById("server-cancel");

const params = new URLSearchParams(window.location.search);
// "?hold" = modo de estilização: nenhum timer/navegação automática.
const holdMode = params.has("hold");

let config: ServerConfig = {
  url: FALLBACK_URL,
  is_default: true,
  default_url: FALLBACK_URL,
};
let autoRetryTimer: number | undefined;
let checking = false;

/** Estado anterior, para o "Cancelar" do formulário voltar para onde estava. */
let stateBeforeForm: "connecting" | "offline" = "connecting";

function scheduleAutoRetry(): void {
  window.clearTimeout(autoRetryTimer);
  if (holdMode) return;
  autoRetryTimer = window.setTimeout(() => {
    void autoCheck();
  }, AUTO_RETRY_MS);
}

function setState(state: "connecting" | "offline" | "server"): void {
  rootEl.dataset.state = state;
  window.clearTimeout(autoRetryTimer);

  if (state === "connecting") {
    statusEl.textContent = "Conectando ao ShvIA…";
  } else if (state === "offline") {
    statusEl.textContent = "Sem conexão com o servidor — reconectando…";
    scheduleAutoRetry();
  } else {
    statusEl.textContent = "Endereço do servidor";
  }
}

/** Mostra qual servidor a casca vai abrir — em connecting e em offline. */
function renderServer(): void {
  // `textContent` e não innerHTML: a URL vem de configuração do usuário, e
  // interpolar isso como markup seria injeção na própria casca — que é a origem
  // PRIVILEGIADA, a única com acesso ao `invoke`.
  serverLabelEl.textContent = config.url.replace(/^https?:\/\//, "");
  serverLabelEl.title = config.url;
  rootEl.dataset.custom = config.is_default ? "no" : "yes";
  if (resetEl) resetEl.hidden = config.is_default;
}

async function loadConfig(): Promise<void> {
  try {
    config = await invoke<ServerConfig>("shvia_server_config");
  } catch {
    // Fora do Tauri (ou comando indisponível): segue com o embutido. Casca que
    // não abre por não ter lido config é pior que casca no servidor padrão.
  }
  renderServer();
}

async function serverReachable(): Promise<boolean> {
  try {
    return await invoke<boolean>("shvia_server_probe", { url: config.url });
  } catch {
    // Sem o comando não há como sondar. Devolver `true` faria a casca navegar às
    // cegas — que é exatamente o comportamento que esta tela existe para evitar.
    return false;
  }
}

function enter(): void {
  window.location.replace(config.url);
}

// Tentativa silenciosa (auto-retry): não mexe na UI enquanto verifica — só
// navega quando o servidor voltar. Sem sobreposição: a próxima só é agendada
// quando esta termina (e só se ainda estivermos offline).
async function autoCheck(): Promise<void> {
  if (checking) return;
  checking = true;
  const ok = await serverReachable();
  checking = false;
  if (ok) {
    enter();
    return;
  }
  if (rootEl.dataset.state === "offline") {
    scheduleAutoRetry();
  }
}

// Tentativa manual/inicial: mostra o estado "connecting" (sonar) enquanto tenta.
async function connect(): Promise<void> {
  setState("connecting");
  if (await serverReachable()) {
    enter();
    return;
  }
  setState("offline");
}

function openForm(): void {
  const atual = rootEl.dataset.state;
  if (atual === "connecting" || atual === "offline") {
    stateBeforeForm = atual;
  }
  errorEl.textContent = "";
  if (inputEl) {
    inputEl.value = config.url;
  }
  setState("server");
  inputEl?.focus();
  inputEl?.select();
}

function closeForm(): void {
  errorEl.textContent = "";
  setState(stateBeforeForm);
  if (stateBeforeForm === "connecting" && !holdMode) {
    void connect();
  }
}

/** Aplica a config nova: renderiza, limpa o erro e tenta entrar. */
function applyAndConnect(novo: ServerConfig): void {
  config = novo;
  renderServer();
  errorEl.textContent = "";
  if (holdMode) {
    setState("server");
    return;
  }
  void connect();
}

async function saveServer(event: Event): Promise<void> {
  event.preventDefault();
  const valor = inputEl?.value ?? "";
  errorEl.textContent = "";
  try {
    // A validação é do RUST, não daqui: é ele que decide o que vira host interno
    // (com pontes nativas), então a regra precisa morar do lado que a aplica.
    applyAndConnect(await invoke<ServerConfig>("shvia_server_set", { url: valor }));
  } catch (e) {
    errorEl.textContent = typeof e === "string" ? e : "Não deu para salvar o endereço.";
  }
}

async function resetServer(): Promise<void> {
  errorEl.textContent = "";
  try {
    applyAndConnect(await invoke<ServerConfig>("shvia_server_reset"));
  } catch {
    errorEl.textContent = "Não deu para voltar ao padrão.";
  }
}

window.addEventListener("DOMContentLoaded", () => {
  retryEl?.addEventListener("click", () => {
    void connect();
  });
  forceEl?.addEventListener("click", enter);
  changeEl?.addEventListener("click", openForm);
  cancelEl?.addEventListener("click", closeForm);
  resetEl?.addEventListener("click", () => {
    void resetServer();
  });
  formEl?.addEventListener("submit", (e) => {
    void saveServer(e);
  });
  // Esc fecha o formulário — o WebView não tem barra de navegação para escapar.
  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && rootEl.dataset.state === "server") {
      closeForm();
    }
  });
  // Rede voltou (evento do SO/navegador) → tenta na hora, sem esperar os 5 s.
  window.addEventListener("online", () => {
    if (!holdMode && rootEl.dataset.state === "offline") {
      window.clearTimeout(autoRetryTimer);
      void autoCheck();
    }
  });

  void (async () => {
    await loadConfig();

    if (params.has("server")) {
      openForm();
      return;
    }
    if (holdMode) {
      setState(params.get("hold") === "offline" ? "offline" : "connecting");
      return;
    }
    void connect();
  })();
});

export {};
