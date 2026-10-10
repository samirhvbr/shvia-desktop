/* The script the agent's browser window runs (navegador-local, ADR-042). One function, called as
 * `(<this file>)(op, args)` by `navegador.rs`; its return value comes back to Rust as JSON through
 * `eval_with_callback`. It has to be SYNCHRONOUS: a Promise comes back as an empty string
 * (measured on WebKitGTK, 10/10/2026), and an exception is swallowed on Windows, so every path
 * returns an object and every failure is `{erro: ...}`.
 *
 * It runs in the PAGE's own world, which the page controls: it can read this script, replace
 * `JSON`, `Array.prototype` and the DOM getters. Nothing here is therefore a security boundary.
 * What it returns is untrusted text; Rust rebuilds the shape from known fields and caps (see
 * `sanear_leitura`), and consent is a native dialog, never something this script decides.
 *
 * Operations: `quem` (where the window is, for the consent dialogs), `ler` (visible text and the
 * list of things a person could click or type into), `descrever` (one numbered element, before
 * the person is asked), `agir` (click or type, after they said yes). */
function (op, a) {
  'use strict';
  try {
    var KEY = Symbol.for('shvia.agente');
    var MAX_TEXTO = 20000;
    var MAX_ELEMENTOS = 150;
    var MAX_ROTULO = 120;
    var MAX_DESTINO = 200;
    // Fields whose name says they hold a secret. Over-blocking is cheap (the person types it
    // themselves); under-blocking puts a password through the model.
    var SEGREDO = /(pass(word|wd)?|senha|pwd|secret|segredo|token|otp|cvv|cvc|\bpin\b|cart[aã]o|card.?n|cc-|one-time|2fa|mfa)/i;
    var TIPOS_DE_TEXTO = /^(text|search|email|url|tel|number)$/i;

    function curto(s, n) {
      s = String(s == null ? '' : s).replace(/\s+/g, ' ').trim();
      return s.length > n ? s.slice(0, n) : s;
    }

    function visivel(el) {
      var r = el.getBoundingClientRect();
      if (r.width < 1 || r.height < 1) return false;
      var cs = window.getComputedStyle(el);
      return cs.visibility !== 'hidden' && cs.display !== 'none' && cs.opacity !== '0';
    }

    function rotulo(el) {
      var r = el.getAttribute('aria-label') || '';
      var ids = el.getAttribute('aria-labelledby');
      if (!r && ids) {
        r = ids.split(/\s+/).map(function (id) {
          var x = document.getElementById(id);
          return x ? (x.innerText || x.textContent || '') : '';
        }).join(' ');
      }
      if (!r && el.id) {
        var l = document.querySelector('label[for="' + window.CSS.escape(el.id) + '"]');
        if (l) r = l.innerText || l.textContent || '';
      }
      if (!r && el.closest) {
        var pai = el.closest('label');
        if (pai) r = pai.innerText || pai.textContent || '';
      }
      if (!r) {
        var t = el.tagName;
        if (t === 'INPUT' && /^(submit|button|reset)$/i.test(el.type)) r = el.value;
        else if (t !== 'INPUT' && t !== 'TEXTAREA' && t !== 'SELECT') r = el.innerText || el.textContent;
      }
      if (!r) r = el.getAttribute('title') || el.getAttribute('placeholder') || el.getAttribute('alt') || el.name || '';
      return curto(r, MAX_ROTULO);
    }

    // Where a link goes, without the query or the fragment: those carry tokens (magic links,
    // session ids) and the model has no use for them.
    function destino(el) {
      if (el.tagName !== 'A') return null;
      try {
        var u = new URL(el.href, window.location.href);
        if (u.protocol !== 'http:' && u.protocol !== 'https:') return curto(u.protocol, 20);
        return curto(u.host + u.pathname, MAX_DESTINO);
      } catch (e) {
        return null;
      }
    }

    function secreto(el) {
      if (el.tagName === 'INPUT' && String(el.type).toLowerCase() === 'password') return true;
      var dicas = [el.name, el.id, el.autocomplete, el.getAttribute('aria-label'), el.getAttribute('placeholder')].join(' ');
      return SEGREDO.test(dicas);
    }

    function editavel(el) {
      if (el.tagName === 'TEXTAREA') return !el.readOnly;
      return el.tagName === 'INPUT' && TIPOS_DE_TEXTO.test(el.type || 'text') && !el.readOnly;
    }

    function impressao(el) {
      return [el.tagName.toLowerCase(), el.type || '', rotulo(el), destino(el) || ''].join('|');
    }

    function descricao(el) {
      return {
        tag: el.tagName.toLowerCase(),
        tipo: el.type ? String(el.type).toLowerCase() : null,
        rotulo: rotulo(el),
        destino: destino(el),
        secreto: secreto(el),
        editavel: editavel(el)
      };
    }

    function achar(gen, ref) {
      var st = window[KEY];
      if (!st || st.gen !== gen) return null;
      var el = st.els[ref];
      return el && el.isConnected ? el : null;
    }

    if (op === 'quem') {
      return { url: window.location.href, titulo: document.title };
    }

    if (op === 'ler') {
      var seletor = 'a[href],button,input,select,textarea,summary,[role=button],[role=link],[role=checkbox],[role=menuitem],[role=tab],[onclick]';
      var els = [];
      var itens = [];
      var total = 0;
      document.querySelectorAll(seletor).forEach(function (el) {
        if (el.disabled || (el.tagName === 'INPUT' && String(el.type).toLowerCase() === 'hidden') || !visivel(el)) return;
        total++;
        if (itens.length >= MAX_ELEMENTOS) return;
        var n = itens.length + 1;
        els[n] = el;
        var d = descricao(el);
        d.ref = n;
        // Whether a field already has something in it, never what: a typed CPF or a saved
        // address is the person's, and the agent does not need it to know what to fill.
        if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') d.preenchido = String(el.value || '') !== '';
        itens.push(d);
      });
      window[KEY] = { gen: a.gen, els: els };
      var texto = ((document.body && document.body.innerText) || '').replace(/\n{3,}/g, '\n\n').trim();
      var corte = texto.length > MAX_TEXTO;
      var u = new URL(window.location.href);
      return {
        url: u.origin + u.pathname,
        titulo: curto(document.title, 200),
        texto: corte ? texto.slice(0, MAX_TEXTO) : texto,
        textoCortado: corte,
        elementos: itens,
        totalElementos: total
      };
    }

    if (op === 'descrever') {
      var alvo = achar(a.gen, a.ref);
      if (!alvo) return { erro: 'desatualizado' };
      var desc = descricao(alvo);
      desc.ok = true;
      desc.url = window.location.href;
      desc.titulo = document.title;
      desc.impressao = impressao(alvo);
      return desc;
    }

    if (op === 'agir') {
      var el2 = achar(a.gen, a.ref);
      if (!el2) return { erro: 'desatualizado' };
      // The element must still be the one the person was shown.
      if (impressao(el2) !== a.impressao) return { erro: 'mudou' };
      if (a.acao === 'clicar') {
        el2.scrollIntoView({ block: 'center', inline: 'center' });
        el2.click();
        return { ok: true };
      }
      if (a.acao === 'digitar') {
        if (secreto(el2)) return { erro: 'campo_secreto' };
        if (!editavel(el2)) return { erro: 'campo_nao_suportado' };
        el2.focus();
        // The native setter, then the events a framework listens to: assigning `.value`
        // alone leaves React and Vue believing the field is still empty.
        var proto = el2.tagName === 'TEXTAREA' ? window.HTMLTextAreaElement.prototype : window.HTMLInputElement.prototype;
        var setter = Object.getOwnPropertyDescriptor(proto, 'value').set;
        setter.call(el2, String(a.texto));
        el2.dispatchEvent(new Event('input', { bubbles: true }));
        el2.dispatchEvent(new Event('change', { bubbles: true }));
        return { ok: true, tamanho: String(a.texto).length };
      }
      return { erro: 'acao_desconhecida' };
    }

    return { erro: 'operacao_desconhecida' };
  } catch (e) {
    return { erro: 'script_falhou', detalhe: String(e && e.message ? e.message : e).slice(0, 200) };
  }
}
