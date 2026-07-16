// Casca de bootstrap do ShvIA Desktop.
//
// A janela Tauri abre esta casca local (instantânea, sem rede) com um splash da
// marca e navega para o ShvIA hospedado — a partir daí a UI é o próprio Blade
// do ShvIA (ADR-002; docs/arquitetura.md). Backport da casca do SHVIA-MOBILE
// (mantê-las em paridade): em vez de navegar às cegas (que estampa a tela de
// erro nativa do WebView quando rede/VPN está fora), a casca VERIFICA o
// servidor antes:
//   1. ping barato no /api/v1/health (no-cors: resposta opaca serve — só
//      queremos saber se o servidor está alcançável; DNS/offline rejeita);
//   2. alcançável → location.replace() (o splash não fica no histórico);
//   3. falhou → estado "offline" com "Tentar agora" (e "Abrir mesmo assim"
//      como escape, caso o ping falhe por outra razão que não a rede).
//
// Offline com AUTO-RETRY: além do botão, a casca fica tentando sozinha a cada
// 5 s e no evento `online` do navegador — voltou a rede, entra sem clique
// (pedido do Samir, 07/07). Isto fecha o item (b) da F2; resta o (a): ler a
// URL do servidor de tauri-plugin-store (config no 1º run).
//
// Dev: "?hold" congela no estado connecting e "?hold=offline" mostra o estado
// offline — pra estilizar o splash sem ser redirecionado (sem auto-retry).

// URL do servidor ShvIA (fonte da verdade). Configurável na F2.
const SHVIA_URL = "https://ia.blue3.com.br";
// Intervalo do auto-retry no estado offline.
const AUTO_RETRY_MS = 5_000;
// Timeout do ping de alcance. O 1º request de rede do WebKitGTK "frio" (processo
// de rede recém-criado no launch) tem um custo fixo de ~5-6 s ANTES de qualquer
// resposta — INDEPENDENTE do modo (medido no webkit2gtk-4.1: no-cors e cors dão
// o mesmo stall; DNS resolve em <10 ms e o servidor responde em <1 ms, então não
// é rede nem servidor — é o cold-start da engine). Depois de aquecido, cai p/
// 50-400 ms. Com 6 s o ping abortava na trave (5,1/5,6/6,3 s medidos) e caía em
// "offline" mesmo com o servidor no ar. 15 s dá folga sobre o custo frio + a
// contenção do launch; offline de verdade rejeita na hora (DNS/rota falha), então
// a folga não pesa no caso comum. Ver docs/decisoes.md (ADR-012).
const REACHABLE_TIMEOUT_MS = 15_000;

const rootEl = document.getElementById("bootstrap")!;
const statusEl = document.getElementById("status")!;
const retryEl = document.getElementById("retry");
const forceEl = document.getElementById("force-open");

// "?hold" = modo de estilização: nenhum timer/navegação automática.
const holdMode = new URLSearchParams(window.location.search).has("hold");

let autoRetryTimer: number | undefined;
let checking = false;

function scheduleAutoRetry(): void {
  window.clearTimeout(autoRetryTimer);
  if (holdMode) return;
  autoRetryTimer = window.setTimeout(() => {
    void autoCheck();
  }, AUTO_RETRY_MS);
}

function setState(state: "connecting" | "offline"): void {
  rootEl.dataset.state = state;
  statusEl.textContent =
    state === "connecting"
      ? "Conectando ao ShvIA…"
      : "Sem conexão com o servidor — reconectando…";
  window.clearTimeout(autoRetryTimer);
  if (state === "offline") {
    scheduleAutoRetry();
  }
}

async function serverReachable(timeoutMs = REACHABLE_TIMEOUT_MS): Promise<boolean> {
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), timeoutMs);
  try {
    await fetch(`${SHVIA_URL}/api/v1/health`, {
      mode: "no-cors",
      cache: "no-store",
      signal: ctl.signal,
    });
    return true;
  } catch {
    return false;
  } finally {
    clearTimeout(timer);
  }
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
    window.location.replace(SHVIA_URL);
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
    window.location.replace(SHVIA_URL);
    return;
  }
  setState("offline");
}

window.addEventListener("DOMContentLoaded", () => {
  retryEl?.addEventListener("click", () => {
    void connect();
  });
  forceEl?.addEventListener("click", () => {
    window.location.replace(SHVIA_URL);
  });
  // Rede voltou (evento do SO/navegador) → tenta na hora, sem esperar os 5 s.
  window.addEventListener("online", () => {
    if (!holdMode && rootEl.dataset.state === "offline") {
      window.clearTimeout(autoRetryTimer);
      void autoCheck();
    }
  });

  if (holdMode) {
    const hold = new URLSearchParams(window.location.search).get("hold");
    setState(hold === "offline" ? "offline" : "connecting");
    return;
  }
  void connect();
});

export {};
