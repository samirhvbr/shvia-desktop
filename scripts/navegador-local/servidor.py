"""Fixture servers for the browser-on-this-machine measurement.

8765: a fake carrier portal (portal/). 8766: a fake ShvIA page that drives the scenario
(controle/) and receives the events at /log. /modo?v=aceitar|recusar sets what the dialog robot
presses. Run by scripts/medir-navegador-local.sh; not part of the app or of CI.
"""
import json, os, sys, threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

AQUI = os.path.dirname(os.path.abspath(__file__))
SAIDA = sys.argv[1]  # work folder: eventos.jsonl, modo.txt


class Estatico(BaseHTTPRequestHandler):
    raiz = ''

    def log_message(self, *a):
        pass

    def do_GET(self):
        u = urlparse(self.path)
        if u.path == '/log':
            q = parse_qs(u.query)
            with open(os.path.join(SAIDA, 'eventos.jsonl'), 'a') as f:
                f.write(json.dumps({'k': q.get('k', [''])[0], 'v': q.get('v', [''])[0]}, ensure_ascii=False) + '\n')
            self.send_response(204); self.end_headers(); return
        if u.path == '/modo':
            with open(os.path.join(SAIDA, 'modo.txt'), 'w') as f:
                f.write(parse_qs(u.query).get('v', ['aceitar'])[0])
            self.send_response(204); self.end_headers(); return
        if u.path == '/cenario':
            corpo = open(os.path.join(SAIDA, 'cenario.txt'), 'rb').read()
            self.send_response(200); self.send_header('Content-Length', str(len(corpo))); self.end_headers()
            self.wfile.write(corpo); return
        caminho = u.path.lstrip('/') or 'index.html'
        arq = os.path.normpath(os.path.join(self.raiz, caminho))
        if not arq.startswith(self.raiz) or not os.path.isfile(arq):
            self.send_response(404); self.end_headers(); return
        corpo = open(arq, 'rb').read()
        self.send_response(200)
        self.send_header('Content-Type', 'text/html; charset=utf-8' if arq.endswith('.html') else 'text/plain')
        self.send_header('Content-Length', str(len(corpo)))
        self.end_headers(); self.wfile.write(corpo)


def servir(porta, pasta):
    h = type('H', (Estatico,), {'raiz': os.path.join(AQUI, pasta)})
    ThreadingHTTPServer(('127.0.0.1', porta), h).serve_forever()


for porta, pasta in ((8765, 'portal'), (8766, 'controle')):
    threading.Thread(target=servir, args=(porta, pasta), daemon=True).start()
threading.Event().wait()
