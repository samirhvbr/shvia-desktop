//! Ponte do Modo Code no **Windows** (WebView2 / Chromium). Espelha o handler
//! `shviaCode` do WebKit (Linux/macOS): registra um `WebMessageReceived` no
//! `ICoreWebView2` que repassa a string postada pela página via
//! `window.chrome.webview.postMessage(...)` para `code_bridge::handle_message`.
//! O caminho Rust→página (`eval`) já é cross-platform, então aqui é só o canal
//! página→Rust. Sem IPC Tauri (ADR-001): canal nativo do WebView2.
//!
//! Versões CASADAS com o wry 0.55 (Cargo.lock: webview2-com 0.38, windows 0.61)
//! — senão os tipos do `ICoreWebView2` não unificam com o ponteiro que o
//! `with_webview` entrega. `IsWebMessageEnabled` é `true` por padrão no WebView2,
//! então `window.chrome.webview` existe sem configuração extra.

use tauri::WebviewWindow;
use webview2_com::take_pwstr;
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2WebMessageReceivedEventArgs;
use webview2_com::WebMessageReceivedEventHandler;
use windows::core::PWSTR;

/// Registra o receptor `WebMessageReceived` no WebView2 da janela (chamado no
/// build). Cada `postMessage(str)` da página vira uma chamada a `handle_message`.
pub fn install(window: &WebviewWindow) {
    let win = window.clone();
    let _ = window.with_webview(move |wv| {
        // No Windows, `controller()` é o `ICoreWebView2Controller` do wry.
        let core = match unsafe { wv.controller().CoreWebView2() } {
            Ok(core) => core,
            Err(_) => return,
        };

        let win_cb = win.clone();
        let handler = WebMessageReceivedEventHandler::create(Box::new(
            move |_webview, args: Option<ICoreWebView2WebMessageReceivedEventArgs>| {
                if let Some(args) = args {
                    let mut msg = PWSTR::null();
                    if unsafe { args.TryGetWebMessageAsString(&mut msg) }.is_ok() {
                        // take_pwstr converte e LIBERA a string alocada pelo WebView2.
                        let text = take_pwstr(msg);
                        if !text.is_empty() {
                            crate::code_bridge::handle_message(&win_cb, &text);
                        }
                    }
                }
                Ok(())
            },
        ));

        // O token de registro do evento é um i64 nesta versão do webview2-com
        // (assinatura `token: *mut i64`); não desregistramos (o handler vive
        // enquanto a janela existe), então descartamos o valor.
        let mut token: i64 = 0;
        unsafe {
            let _ = core.add_WebMessageReceived(&handler, &mut token);
        }
    });
}
