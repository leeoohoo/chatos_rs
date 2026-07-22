// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const assert = require('node:assert/strict');
const test = require('node:test');

const { coreRestartDelayMs } = require('../electron/core-runtime.cjs');

test('backs off unexpected Core restarts without growing past five seconds', () => {
  assert.deepEqual(
    [0, 1, 2, 3, 4, 5, 20].map(coreRestartDelayMs),
    [250, 500, 1_000, 2_000, 4_000, 5_000, 5_000],
  );
});
