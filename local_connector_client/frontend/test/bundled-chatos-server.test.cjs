// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const assert = require('node:assert/strict');
const fs = require('node:fs');
const http = require('node:http');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');

const {
  startBundledChatosServer,
} = require('../electron/bundled-chatos-server.cjs');

function request(url, options = {}) {
  return new Promise((resolve, reject) => {
    const req = http.request(url, options, (response) => {
      const chunks = [];
      response.on('data', (chunk) => chunks.push(chunk));
      response.on('end', () => resolve({
        body: Buffer.concat(chunks).toString('utf8'),
        headers: response.headers,
        status: response.statusCode,
      }));
    });
    req.on('error', reject);
    req.end();
  });
}

test('serves bundled assets and SPA routes from loopback only', async (context) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'chatos-bundled-frontend-'));
  fs.mkdirSync(path.join(root, 'assets'));
  fs.writeFileSync(path.join(root, 'index.html'), '<html>bundled chatos</html>');
  fs.writeFileSync(path.join(root, 'assets', 'app.js'), 'export const ready = true;');
  context.after(() => fs.rmSync(root, { recursive: true, force: true }));

  const server = await startBundledChatosServer(root);
  context.after(() => server.close());

  const index = await request(`${server.origin}/`);
  assert.equal(index.status, 200);
  assert.match(index.body, /bundled chatos/);
  assert.equal(index.headers['cache-control'], 'no-store');

  const asset = await request(`${server.origin}/assets/app.js`);
  assert.equal(asset.status, 200);
  assert.match(asset.body, /ready = true/);
  assert.match(asset.headers['cache-control'], /immutable/);

  const spaRoute = await request(`${server.origin}/projects/local-1`, {
    headers: { Accept: 'text/html' },
  });
  assert.equal(spaRoute.status, 200);
  assert.match(spaRoute.body, /bundled chatos/);

  const missingAsset = await request(`${server.origin}/assets/missing.js`);
  assert.equal(missingAsset.status, 404);

  const invalidPath = await request(`${server.origin}/%5Coutside`);
  assert.equal(invalidPath.status, 400);

  const invalidHost = await request(server.origin, {
    headers: { Host: 'attacker.example' },
  });
  assert.equal(invalidHost.status, 403);

  const invalidMethod = await request(server.origin, { method: 'POST' });
  assert.equal(invalidMethod.status, 405);
});
