#!/usr/bin/env python3
"""What the installed WebKitGTK exposes for the microphone and for pasting an image (1.6.45).

ADR-008 (30/06/2026) recorded that the microphone does not capture and an image does not paste
on Linux. 0.4.8 (02/07) added a paste bridge (CLIPBOARD_IMAGE_PASTE_JS in lib.rs) and nobody
measured again. This probe answers the part that needs no person at the keyboard: an offscreen
WebView (no window appears) with the same settings `configure_linux_webview` turns on, and a
page on the server's origin that reports:
  - secure context, getUserMedia, and the kinds enumerateDevices lists;
  - navigator.clipboard.read and ClipboardItem;
  - whether a synthetic `paste` carrying an image File reaches a listener with its item: the
    half of the bridge that re-dispatches.

It never opens the microphone and never touches the clipboard: every permission request is
DENIED, getUserMedia is not called, and the image is built in the page. Whether a real
microphone captures and a real Ctrl+V reads the system clipboard needs a person:
docs/roteiro-microfone-e-colar.md, two minutes.

Needs python3-gi and the WebKit2 4.1 typelib (gir1.2-webkit2-4.1), and a display (DISPLAY).
Usage: python3 scripts/sonda-webkitgtk.py
"""
import gi, sys
gi.require_version('Gtk', '3.0'); gi.require_version('WebKit2', '4.1')
from gi.repository import Gtk, WebKit2, GLib
settings = WebKit2.Settings()
for k in ['enable-media-stream', 'enable-mediasource', 'enable-webrtc', 'javascript-can-access-clipboard']:
    settings.set_property(k, True)
ucm = WebKit2.UserContentManager()
ucm.register_script_message_handler('probe')
view = WebKit2.WebView.new_with_user_content_manager(ucm)
view.set_settings(settings)
pedidos = []
def on_perm(v, req):
    pedidos.append(type(req).__name__); req.deny(); return True
view.connect('permission-request', on_perm)
def on_msg(m, res):
    import gi.repository.WebKit2 as W
    print(f'WebKitGTK {W.get_major_version()}.{W.get_minor_version()}.{W.get_micro_version()}')
    print(res.get_js_value().to_json(2)); print('permission requests (all denied):', pedidos); Gtk.main_quit()
ucm.connect('script-message-received::probe', on_msg)
win = Gtk.OffscreenWindow(); win.add(view); win.show_all()
html = """<body><script>
(async () => {
  const r = {};
  r.webkit = (navigator.userAgent.match(/AppleWebKit\\/[\\d.]+/) || [''])[0];
  r.secureContext = window.isSecureContext;
  r.getUserMedia = !!(navigator.mediaDevices && navigator.mediaDevices.getUserMedia);
  try { const d = await navigator.mediaDevices.enumerateDevices(); r.enumerateDevices = d.map(x => x.kind); }
  catch (e) { r.enumerateDevices = 'error: ' + e; }
  r.clipboardRead = !!(navigator.clipboard && navigator.clipboard.read);
  r.ClipboardItem = typeof ClipboardItem;
  try {
    const f = new File([new Uint8Array([137, 80, 78, 71])], 'x.png', {type: 'image/png'});
    const dt = new DataTransfer(); dt.items.add(f);
    let seen = 'listener never ran';
    document.addEventListener('paste', e => {
      const c = e.clipboardData;
      seen = c ? {items: c.items.length, firstType: c.items[0] ? c.items[0].type : null, files: c.files.length} : 'clipboardData is null';
    }, {once: true});
    document.body.dispatchEvent(new ClipboardEvent('paste', {clipboardData: dt, bubbles: true, cancelable: true}));
    r.syntheticPasteWithImage = seen;
  } catch (e) { r.syntheticPasteWithImage = 'error: ' + e; }
  window.webkit.messageHandlers.probe.postMessage(r);
})();
</script></body>"""
# The page is given the server's origin, so it is a secure context, as the real page is.
view.load_html(html, 'https://ai.shvia.org/')
GLib.timeout_add_seconds(20, lambda: (print('TIMEOUT'), Gtk.main_quit()))
Gtk.main()
