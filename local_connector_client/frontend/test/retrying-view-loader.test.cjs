// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const assert = require('node:assert/strict');
const { EventEmitter } = require('node:events');
const test = require('node:test');

const { attachRetryingViewLoader } = require('../electron/retrying-view-loader.cjs');

class FakeWebContents extends EventEmitter {
  isDestroyed() {
    return false;
  }
}

test('retries a failed developer page load without stacking timers', async () => {
  const webContents = new FakeWebContents();
  const scheduled = [];
  let loadCount = 0;
  const loader = attachRetryingViewLoader({
    webContents,
    load: async () => {
      loadCount += 1;
      if (loadCount === 1) {
        throw new Error('connection refused');
      }
    },
    shouldRetry: () => true,
    setTimer: (callback) => {
      scheduled.push(callback);
      return callback;
    },
    clearTimer: (callback) => {
      const index = scheduled.indexOf(callback);
      if (index >= 0) {
        scheduled.splice(index, 1);
      }
    },
  });

  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(loadCount, 1);
  assert.equal(scheduled.length, 1);

  webContents.emit('did-fail-load', {}, -102, 'Connection refused', 'http://127.0.0.1:8088', true);
  assert.equal(scheduled.length, 1);
  scheduled.shift()();
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(loadCount, 2);

  loader.dispose();
});

test('does not retry normal navigation cancellation or production loads', () => {
  const webContents = new FakeWebContents();
  const scheduled = [];
  const loader = attachRetryingViewLoader({
    webContents,
    load: () => undefined,
    shouldRetry: () => false,
    setTimer: (callback) => {
      scheduled.push(callback);
      return callback;
    },
    clearTimer: () => {},
  });

  webContents.emit('did-fail-load', {}, -3, 'Aborted', 'http://127.0.0.1:8088', true);
  webContents.emit('did-fail-load', {}, -102, 'Connection refused', 'http://127.0.0.1:8088', true);
  assert.equal(scheduled.length, 0);

  loader.dispose();
});
