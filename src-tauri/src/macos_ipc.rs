//! Ponte do Modo Code no **macOS** (WKWebView). Espelha o handler `shviaCode`
//! do Linux (WebKitGTK): registra um `WKScriptMessageHandler` que repassa a
//! mensagem da página pro `code_bridge::handle_message`. O caminho Rust→página
//! (`eval`) já é cross-platform, então aqui é só o canal página→Rust.
//!
//! Escrito para **objc2 0.6 / objc2-web-kit 0.3** (versões que o wry 0.55
//! resolve). O `WKScriptMessageHandler` exige `MainThreadOnly`, então a classe é
//! declarada como tal (`#[thread_kind = MainThreadOnly]`) e criada com o
//! `MainThreadMarker` do `with_webview` (que roda na main thread). Sem IPC Tauri
//! (ADR-001): canal nativo do WebKit, como no Linux.

use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_foundation::NSString;
use objc2_web_kit::{WKScriptMessage, WKScriptMessageHandler, WKUserContentController, WKWebView};
use tauri::WebviewWindow;

struct Ivars {
    window: WebviewWindow,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ShviaCodeMsgHandler"]
    #[ivars = Ivars]
    struct Handler;

    unsafe impl NSObjectProtocol for Handler {}

    unsafe impl WKScriptMessageHandler for Handler {
        #[unsafe(method(userContentController:didReceiveScriptMessage:))]
        fn did_receive(&self, _controller: &WKUserContentController, message: &WKScriptMessage) {
            let body = unsafe { message.body() };
            if let Ok(text) = body.downcast::<NSString>() {
                crate::code_bridge::handle_message(&self.ivars().window, &text.to_string());
            }
        }
    }
);

impl Handler {
    fn new(mtm: MainThreadMarker, window: WebviewWindow) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars { window });
        unsafe { msg_send![super(this), init] }
    }
}

/// Registra o handler `shviaCode` no WKWebView da janela (chamado no build).
pub fn install(window: &WebviewWindow) {
    let win = window.clone();
    let _ = window.with_webview(move |wv| {
        let mtm = MainThreadMarker::new().expect("with_webview roda na main thread");
        // wv.inner() = ponteiro do WKWebView do wry.
        let webview: &WKWebView = unsafe { &*(wv.inner() as *mut WKWebView) };
        let ucc: Retained<WKUserContentController> =
            unsafe { webview.configuration().userContentController() };
        let handler = Handler::new(mtm, win.clone());
        let proto = ProtocolObject::from_ref(&*handler);
        unsafe { ucc.addScriptMessageHandler_name(proto, &NSString::from_str("shviaCode")) };
    });
}
