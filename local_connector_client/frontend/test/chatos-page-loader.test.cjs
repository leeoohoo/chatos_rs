// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const assert = require('node:assert/strict');
const test = require('node:test');

const {
  loadChatosPage,
  reloadChatosPage,
} = require('../electron/chatos-page-loader.cjs');

test('developer page loads clear HTTP cache before requesting the local frontend', async () => {
  const calls = [];
  const webContents = {
    session: {
      clearCache: async () => calls.push('clear-cache'),
    },
    loadURL: async (url) => calls.push(`load:${url}`),
  };

  await loadChatosPage({
    webContents,
    url: 'http://127.0.0.1:8088/?desktop=local-connector',
    bypassCache: true,
  });

  assert.deepEqual(calls, [
    'clear-cache',
    'load:http://127.0.0.1:8088/?desktop=local-connector',
  ]);
});

test('hosted production page loads keep normal browser caching', async () => {
  let cacheClearCount = 0;
  const webContents = {
    session: {
      clearCache: async () => {
        cacheClearCount += 1;
      },
    },
    loadURL: async () => undefined,
  };

  await loadChatosPage({
    webContents,
    url: 'https://app.jgoool.com/',
    bypassCache: false,
  });

  assert.equal(cacheClearCount, 0);
});

test('developer refresh bypasses cache without clearing login storage', () => {
  const calls = [];
  const webContents = {
    reload: () => calls.push('reload'),
    reloadIgnoringCache: () => calls.push('reload-ignoring-cache'),
  };

  reloadChatosPage({ webContents, bypassCache: true });
  reloadChatosPage({ webContents, bypassCache: false });

  assert.deepEqual(calls, ['reload-ignoring-cache', 'reload']);
});
