// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const test = require('node:test');
const assert = require('node:assert/strict');

const {
  createDesktopTicketAuthenticator,
} = require('../electron/desktop-auth.cjs');

test('desktop ticket authentication waits for a successful Core response', async () => {
  const calls = [];
  const authenticate = createDesktopTicketAuthenticator({
    sendIpcHttpRequest: async (request) => {
      calls.push(request);
      return {
        status: 200,
        ok: true,
        body: JSON.stringify({ configured: true, connector_running: true }),
      };
    },
    localApiHeaders: (hasBody) => ({ Authorization: 'Bearer desktop', hasBody }),
    getCloudBaseUrl: () => 'http://127.0.0.1:39230',
  });

  const status = await authenticate(' ticket-1 ');

  assert.equal(status.configured, true);
  assert.equal(status.connector_running, true);
  assert.deepEqual(calls, [{
    endpoint: '/api/local/auth/desktop-ticket',
    method: 'POST',
    headers: { Authorization: 'Bearer desktop', hasBody: true },
    body: JSON.stringify({
      cloud_base_url: 'http://127.0.0.1:39230',
      ticket: 'ticket-1',
    }),
  }]);
});

test('desktop ticket authentication rejects a failed Core response', async () => {
  const authenticate = createDesktopTicketAuthenticator({
    sendIpcHttpRequest: async () => ({
      status: 401,
      ok: false,
      body: JSON.stringify({ detail: 'ticket expired' }),
    }),
    localApiHeaders: () => ({}),
    getCloudBaseUrl: () => 'http://127.0.0.1:39230',
  });

  await assert.rejects(() => authenticate('ticket-2'), /ticket expired/);
});

test('desktop ticket authentication retries while the Core IPC socket is starting', async () => {
  let attempts = 0;
  const authenticate = createDesktopTicketAuthenticator({
    sendIpcHttpRequest: async () => {
      attempts += 1;
      if (attempts < 3) {
        const error = new Error('connect ENOENT /tmp/chatos/core.sock');
        error.code = 'ENOENT';
        throw error;
      }
      return {
        status: 200,
        ok: true,
        body: JSON.stringify({ configured: true }),
      };
    },
    localApiHeaders: () => ({}),
    getCloudBaseUrl: () => 'http://127.0.0.1:39230',
    retryAttempts: 5,
    retryDelayMs: 1,
  });

  const status = await authenticate('ticket-startup-race');

  assert.equal(status.configured, true);
  assert.equal(attempts, 3);
});

test('desktop ticket authentication does not retry non-transient IPC failures', async () => {
  let attempts = 0;
  const authenticate = createDesktopTicketAuthenticator({
    sendIpcHttpRequest: async () => {
      attempts += 1;
      const error = new Error('permission denied');
      error.code = 'EACCES';
      throw error;
    },
    localApiHeaders: () => ({}),
    getCloudBaseUrl: () => 'http://127.0.0.1:39230',
    retryAttempts: 5,
    retryDelayMs: 1,
  });

  await assert.rejects(() => authenticate('ticket-3'), /permission denied/);
  assert.equal(attempts, 1);
});

test('desktop ticket authentication rejects an empty ticket before contacting Core', async () => {
  let called = false;
  const authenticate = createDesktopTicketAuthenticator({
    sendIpcHttpRequest: async () => {
      called = true;
      return { status: 200, ok: true, body: '{}' };
    },
    localApiHeaders: () => ({}),
    getCloudBaseUrl: () => 'http://127.0.0.1:39230',
  });

  await assert.rejects(() => authenticate('  '), /ticket is empty/);
  assert.equal(called, false);
});
