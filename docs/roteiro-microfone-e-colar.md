# Microphone and image paste on Linux: the two-minute check (the owner runs it)

> **Why this exists.** [ADR-008](decisoes.md#adr-008--webkitgtk-microfone-e-paste-de-imagem-têm-limite-no-linux)
> (30/06/2026) found that, on WebKitGTK, the microphone does not capture and an image does not
> paste. Two days later **0.4.8 added a paste bridge** (`CLIPBOARD_IMAGE_PASTE_JS` in `lib.rs`),
> and nobody measured again. The docs kept saying "does not work" for three months. The owner
> answered "re-measure, and work around it if it fails" on 23/09/2026.

## What was measured without a person (23/09/2026, 1.6.45)

`scripts/sonda-webkitgtk.py` loads an offscreen WebView (no window appears) with the app's
settings, on the server's origin. It denies every permission, never opens the microphone, and
never touches the clipboard. On this machine (Debian 13, **WebKitGTK 2.52.6**):

| what | result |
|---|---|
| secure context | yes |
| `navigator.mediaDevices.getUserMedia` | exposed |
| `enumerateDevices()` | an `audioinput` and a `videoinput` |
| `navigator.clipboard.read`, `ClipboardItem` | exposed |
| a synthetic `paste` carrying an image `File` (the bridge's second half) | **delivered**: the listener sees 1 item, `image/png`, 1 file |
| GStreamer capture elements (`pulsesrc`, `pipewiresrc`, `autoaudiosrc`) | installed |

Two corrections to earlier readings. First, `gst-inspect-1.0` is not installed here, so
checking the elements with it reports every one as absent, a false zero. They were checked
through GObject introspection instead. Second, the same probe **with the app's settings left
at their defaults** gives the same result: in 2.52.6, `enable-media-stream` is on by default and
the async clipboard API does not depend on `javascript-can-access-clipboard`. Exposure is no
longer the question.

**What only a person can answer:** does the microphone capture once permission is granted, and
does a real Ctrl+V read an image from the system clipboard? That is the check below.

## The check (two minutes, Linux, the packaged app)

| # | Do | Expected | ✅/❌, and what happened |
|---|---|---|---|
| 1 | Put an image on the clipboard (a screenshot "copied to clipboard", or *Copy image* in a browser). In the app, click the chat's input and press **Ctrl+V** | The image is attached to the message | |
| 2 | Click the chat's **microphone** button, allow it if asked, speak for five seconds, stop | The recording reacts to your voice, and the words come back as text | |
| 3 | Optional: `python3 scripts/sonda-webkitgtk.py`, and paste its output here | It records which WebKitGTK the result belongs to | |

For a ❌, write down what happened: nothing at all, text pasted instead of the image, an error,
or no permission prompt.

## If it fails: the native workarounds

- **Paste fails:** read the image from the system clipboard in Rust on Ctrl+V (the Tauri
  clipboard-manager plugin, or `arboard`) and hand it to the page through the same synthetic
  `paste` the bridge already dispatches. The dispatch half is proven above; only the read half
  would change.
- **Microphone fails:** capture in Rust (`cpal`) and stream the audio to the page. That is a
  larger piece, because the page's recorder expects a `MediaStream`. ADR-008's other exit
  (Electron) stays the product-level alternative.
