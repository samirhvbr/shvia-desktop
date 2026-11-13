'use strict';
// Trust the fixture CA explicitly; do not change a host trust store or use insecure TLS.
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const https = require('node:https');
const dir = process.env.RUN_DIR;
if (!dir) throw new Error('RUN_DIR must name the disposable fixture');
const state = JSON.parse(fs.readFileSync(path.join(dir, 'server-state.json')));
const ca = fs.readFileSync(path.join(dir, 'ca.crt'));
async function check(destination, origin) {
  const value = await new Promise((resolve, reject) => {
    const url = new URL('/status', origin);
    const request = (url.protocol === 'https:' ? https : http).get(url, { ca }, response => {
      let body = '';
      response.on('data', chunk => { body += chunk; });
      response.on('end', () => {
        try {
          if (response.statusCode !== 200) throw new Error('HTTP ' + response.statusCode);
          resolve(JSON.parse(body));
        } catch (error) { reject(error); }
      });
    });
    request.setTimeout(5000, () => request.destroy(new Error('Fixture preflight timeout')));
    request.on('error', reject);
  });
  if (value.probeReceivedOrigin !== origin) throw new Error('Wrong destination: ' + origin);
  return { destination, origin, httpStatus: 200, receivedOrigin: value.probeReceivedOrigin, role: value.probeRole };
}
(async () => {
  const results = [];
  for (const [destination, origin] of Object.entries(state.targets)) results.push(await check(destination, origin));
  fs.writeFileSync(path.join(dir, 'destination-preflight.json'), JSON.stringify(results, null, 2));
  console.log(JSON.stringify(results, null, 2));
})().catch(error => { console.error(error.message); process.exitCode = 1; });
