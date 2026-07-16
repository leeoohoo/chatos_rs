// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const assert = require('node:assert/strict');
const test = require('node:test');
const path = require('node:path');
const { pathToFileURL } = require('node:url');

const { isTrustedMainFrameEvent } = require('../electron/ipc-trust.cjs');

function eventFixture({ id = 7, frameUrl = 'http://127.0.0.1:41000/' } = {}) {
  const mainFrame = { url: frameUrl };
  const sender = { id, mainFrame };
  return {
    sender,
    senderFrame: mainFrame,
  };
}

test('accepts only trusted main-frame events from the expected origin', () => {
  const trustedIds = new Set([7]);
  const event = eventFixture();

  assert.equal(
    isTrustedMainFrameEvent(event, trustedIds, 'http://127.0.0.1:41000'),
    true,
  );
  assert.equal(
    isTrustedMainFrameEvent(eventFixture({ id: 8 }), trustedIds),
    false,
  );
  assert.equal(
    isTrustedMainFrameEvent(
      eventFixture({ frameUrl: 'https://app.jgoool.com/' }),
      trustedIds,
      'http://127.0.0.1:41000',
    ),
    false,
  );

  const subframeEvent = eventFixture();
  subframeEvent.senderFrame = { url: 'http://127.0.0.1:41000/iframe' };
  assert.equal(isTrustedMainFrameEvent(subframeEvent, trustedIds), false);
});

test('supports exact local-document validation for trusted WebContents', () => {
  const trustedIds = new Set([7]);
  const indexPath = path.resolve('/application/dist/index.html');
  const localUrl = pathToFileURL(indexPath);
  localUrl.searchParams.set('view', 'shell');
  const event = eventFixture({ frameUrl: localUrl.toString() });

  assert.equal(
    isTrustedMainFrameEvent(
      event,
      trustedIds,
      (url) => url === localUrl.toString(),
    ),
    true,
  );
  assert.equal(
    isTrustedMainFrameEvent(
      eventFixture({ frameUrl: 'https://example.com/' }),
      trustedIds,
      (url) => url === localUrl.toString(),
    ),
    false,
  );
});
