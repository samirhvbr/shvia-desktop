// Casca de bootstrap do ShvIA Desktop.
//
// A janela Tauri abre esta casca local (instantânea, sem rede) só para mostrar
// um splash com a marca enquanto o WebView navega para o ShvIA hospedado. A
// partir daí a UI é o próprio Blade do ShvIA (mesmas funções). Ver
// docs/arquitetura.md (fluxo de auth) e docs/decisoes.md (ADR-002, ADR-005).
//
// F2 vai: (a) ler a URL do servidor de tauri-plugin-store (config no 1º run) em
// vez da constante abaixo; (b) capturar falha de navegação (offline) e exibir
// uma tela de retry com a marca, em vez do erro nativo do WebView.

// URL padrão do servidor ShvIA (fonte da verdade). Configurável na F2.
const SHVIA_URL = "https://ia.blue3.com.br";

window.addEventListener("DOMContentLoaded", () => {
  // replace() para o splash local não ficar no histórico de navegação.
  window.location.replace(SHVIA_URL);
});

export {};
