// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const assert = require('node:assert/strict');
const test = require('node:test');

const {
  approvalOverlayBounds,
  normalizeApprovalOverlayMode,
} = require('../electron/approval-overlay-layout.cjs');

test('approval overlay is hidden for unknown and hidden modes', () => {
  assert.equal(normalizeApprovalOverlayMode('anything'), 'hidden');
  assert.equal(approvalOverlayBounds({ width: 1180, height: 780 }, 'hidden'), null);
});

test('approval overlay anchors compact and expanded surfaces to the top right', () => {
  assert.deepEqual(
    approvalOverlayBounds({ width: 1180, height: 780 }, 'compact'),
    { x: 892, y: 68, width: 276, height: 68 },
  );
  assert.deepEqual(
    approvalOverlayBounds({ width: 1180, height: 780 }, 'expanded'),
    { x: 708, y: 68, width: 460, height: 560 },
  );
});

test('approval overlay remains inside a narrow window', () => {
  assert.deepEqual(
    approvalOverlayBounds({ width: 360, height: 420 }, 'expanded'),
    { x: 12, y: 68, width: 336, height: 340 },
  );
});
