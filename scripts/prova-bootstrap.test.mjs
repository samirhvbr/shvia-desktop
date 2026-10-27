import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { transformWithOxc } from 'vite';

// Run the shipped shell with controlled native replies and timers. Only the Tauri
// import is replaced; the event handlers and state transitions are production code.
const source = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
assert.match(source, /import \{ invoke \} from "@tauri-apps\/api\/core";/);
const { code } = await transformWithOxc(source.replace(
  /import \{ invoke \} from "@tauri-apps\/api\/core";/,
  '',
), 'main.ts');
const flush = () => new Promise(resolve => setImmediate(resolve));
const initialUrl = 'https://first.example';
const nextUrl = 'https://second.example';
const config = url => ({ url, is_default: false, default_url: initialUrl });

async function shell(search = '') {
  class Element extends EventTarget {
    dataset = {};
    value = '';
    textContent = '';
    focus() {}
    select() {}
  }
  const elements = new Map();
  const element = id => {
    if (!elements.has(id)) elements.set(id, new Element());
    return elements.get(id);
  };
  element('bootstrap').dataset.state = 'connecting';
  const window = new EventTarget();
  const navigations = [];
  const probes = [];
  const timers = new Map();
  let timerId = 0;
  Object.assign(window, {
    location: { search, replace: url => navigations.push(url) },
    setTimeout: callback => { timers.set(++timerId, callback); return timerId; },
    clearTimeout: id => timers.delete(id),
  });
  const invoke = async (command, args) => {
    if (command === 'shvia_server_config') return config(initialUrl);
    if (command === 'shvia_server_set') return config(args.url);
    if (command === 'shvia_server_probe') {
      return new Promise(resolve => probes.push({ url: args.url, resolve }));
    }
    throw new Error(`Unexpected command: ${command}`);
  };
  runInNewContext(code.replace(/export\s*\{\s*\};?/, ''), {
    window, document: { getElementById: element }, URLSearchParams, invoke,
  });
  window.dispatchEvent(new Event('DOMContentLoaded'));
  await flush();
  return {
    element, navigations, probes, timers,
    state: () => element('bootstrap').dataset.state,
    async click(id) { element(id).dispatchEvent(new Event('click')); await flush(); },
    async reply(index, reachable) { probes[index].resolve(reachable); await flush(); },
    async submit(url) {
      element('server-url').value = url;
      element('server-form').dispatchEvent(new Event('submit', { cancelable: true }));
      await flush();
    },
    async retryTimer() {
      assert.equal(timers.size, 1);
      const [id, callback] = timers.entries().next().value;
      timers.delete(id);
      callback();
      await flush();
    },
  };
}

test('a reachable server opens the allowed deep-link destination', async () => {
  const app = await shell('?ir=%2Fchat%3Fid%3D7');
  await app.reply(0, true);
  assert.deepEqual(app.navigations, [`${initialUrl}/chat?id=7`]);
});

for (const reachable of [false, true]) {
  test(`opening server settings ignores the pending startup reply (${reachable})`, async () => {
    const app = await shell();
    await app.click('server-change');
    app.element('server-url').value = nextUrl;
    await app.reply(0, reachable);
    assert.equal(app.state(), 'server');
    assert.equal(app.element('server-url').value, nextUrl);
    assert.deepEqual(app.navigations, []);
    assert.equal(app.timers.size, 0);
  });

  test(`opening server settings ignores the pending automatic retry (${reachable})`, async () => {
    const app = await shell();
    await app.reply(0, false);
    await app.retryTimer();
    await app.click('server-change');
    await app.reply(1, reachable);
    assert.equal(app.state(), 'server');
    assert.deepEqual(app.navigations, []);
    assert.equal(app.timers.size, 0);
  });

  test(`saving a new server waits for its own probe (${reachable})`, async () => {
    const app = await shell();
    await app.click('server-change');
    await app.submit(nextUrl);
    assert.deepEqual(app.probes.map(probe => probe.url), [initialUrl, nextUrl]);
    await app.reply(0, reachable);
    assert.equal(app.state(), 'connecting');
    assert.deepEqual(app.navigations, []);
    assert.equal(app.timers.size, 0);
    await app.reply(1, true);
    assert.deepEqual(app.navigations, [nextUrl]);
  });
}

test('canceling settings starts a fresh check even while the old check is pending', async () => {
  const app = await shell();
  await app.click('server-change');
  await app.click('server-cancel');
  assert.equal(app.probes.length, 2);
  await app.reply(0, false);
  assert.equal(app.state(), 'connecting');
  await app.reply(1, true);
  assert.deepEqual(app.navigations, [initialUrl]);
});

test('canceling settings from offline resumes automatic retries', async () => {
  const app = await shell();
  await app.reply(0, false);
  await app.retryTimer();
  await app.click('server-change');
  await app.click('server-cancel');
  await app.retryTimer();
  assert.equal(app.probes.length, 3);
  await app.reply(1, true);
  assert.deepEqual(app.navigations, []);
  await app.reply(2, false);
  assert.equal(app.state(), 'offline');
  assert.equal(app.timers.size, 1);
  await app.retryTimer();
  await app.reply(3, true);
  assert.deepEqual(app.navigations, [initialUrl]);
});

test('manual retry supersedes an automatic retry in flight', async () => {
  const app = await shell();
  await app.reply(0, false);
  await app.retryTimer();
  await app.click('retry');
  await app.reply(1, true);
  assert.deepEqual(app.navigations, []);
  await app.reply(2, false);
  assert.equal(app.state(), 'offline');
  assert.equal(app.timers.size, 1);
});
