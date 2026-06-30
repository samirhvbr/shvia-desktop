//! ShvIA Desktop — shell fino Tauri 2.
//!
//! A janela abre a casca local (`src/`), que mostra um splash com a marca e
//! redireciona o WebView para o ShvIA hospedado (`https://ia.blue3.com.br`).
//! A partir daí a UI é o próprio Blade do ShvIA — "mesmas funções" (ADR-002).
//!
//! Postura de menor privilégio: **nenhum comando nativo é exposto à página
//! remota na F1**. O servidor é a fonte da verdade; o cliente não abre banco
//! nem guarda segredo. Detalhes em `docs/arquitetura.md` e `docs/decisoes.md`.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .run(tauri::generate_context!())
        .expect("erro ao executar o aplicativo ShvIA Desktop");
}
