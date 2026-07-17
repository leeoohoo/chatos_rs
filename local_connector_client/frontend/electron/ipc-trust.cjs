// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

function isTrustedMainFrameEvent(event, trustedWebContentsIds, expectedLocation = null) {
  const sender = event?.sender;
  const senderFrame = event?.senderFrame;
  if (!sender || !trustedWebContentsIds.has(sender.id)) {
    return false;
  }
  if (!senderFrame || senderFrame !== sender.mainFrame) {
    return false;
  }
  if (!expectedLocation) {
    return true;
  }
  if (typeof expectedLocation === 'function') {
    return Boolean(expectedLocation(senderFrame.url));
  }
  try {
    return new URL(senderFrame.url).origin === expectedLocation;
  } catch {
    return false;
  }
}

module.exports = {
  isTrustedMainFrameEvent,
};
