// Casca de bootstrap do ShvIA Desktop.
//
// A janela Tauri abre esta casca local (instantânea, sem rede) com um splash da
// marca e navega para o ShvIA hospedado — a partir daí a UI é o próprio Blade
// do ShvIA (ADR-002; docs/arquitetura.md). Backport da casca do SHVIA-MOBILE
// (0.2.2): em vez de navegar às cegas (que estampa a tela de erro nativa do
// WebView quando rede/VPN está fora), a casca VERIFICA o servidor antes:
//   1. ping barato no /api/v1/health (no-cors: resposta opaca serve — só
//      queremos saber se o servidor está alcançável; DNS/offline rejeita);
//   2. alcançável → location.replace() (o splash não fica no histórico);
//   3. falhou → estado "offline" com "Tentar novamente" (e "Abrir mesmo assim"
//      como escape, caso o ping falhe por outra razão que não a rede).
// Isto entrega o item (b) da F2; resta o (a): ler a URL do servidor de
// tauri-plugin-store (config no 1º run) em vez da constante abaixo.
//
// Dev: "?hold" congela no estado connecting e "?hold=offline" mostra o estado
// offline — pra estilizar o splash sem ser redirecionado.

// URL do servidor ShvIA (fonte da verdade). Configurável na F2.
const SHVIA_URL = "https://ia.blue3.com.br";

const rootEl = document.getElementById("bootstrap")!;
const statusEl = document.getElementById("status")!;
const retryEl = document.getElementById("retry");
const forceEl = document.getElementById("force-open");

function setState(state: "connecting" | "offline"): void {
  rootEl.dataset.state = state;
  statusEl.textContent =
    state === "connecting" ? "Conectando ao ShvIA…" : "Sem conexão com o servidor.";
}

async function serverReachable(timeoutMs = 6000): Promise<boolean> {
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

  const hold = new URLSearchParams(window.location.search).get("hold");
  if (hold !== null) {
    setState(hold === "offline" ? "offline" : "connecting");
    return;
  }
  void connect();
});

export {};
