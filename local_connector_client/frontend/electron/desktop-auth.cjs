// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

function responseErrorMessage(response) {
  const fallback = `Local Connector login synchronization failed (HTTP ${response?.status || 0})`;
  const raw = typeof response?.body === 'string' ? response.body.trim() : '';
  if (!raw) {
    return fallback;
  }
  try {
    const payload = JSON.parse(raw);
    const detail = typeof payload?.detail === 'string' ? payload.detail.trim() : '';
    const message = typeof payload?.message === 'string' ? payload.message.trim() : '';
    const error = typeof payload?.error === 'string' ? payload.error.trim() : '';
    return detail || message || error || fallback;
  } catch {
    return raw || fallback;
  }
}

function parsedSuccessBody(response) {
  const raw = typeof response?.body === 'string' ? response.body.trim() : '';
  if (!raw) {
    return { ok: true };
  }
  try {
    return JSON.parse(raw);
  } catch {
    return { ok: true };
  }
}

function createDesktopTicketAuthenticator({
  sendIpcHttpRequest,
  localApiHeaders,
  getCloudBaseUrl,
}) {
  return async function authenticateDesktopTicket(ticket) {
    const trimmed = String(ticket || '').trim();
    if (!trimmed) {
      throw new Error('Local Connector authorization ticket is empty');
    }
    const response = await sendIpcHttpRequest({
      endpoint: '/api/local/auth/desktop-ticket',
      method: 'POST',
      headers: localApiHeaders(true),
      body: JSON.stringify({
        cloud_base_url: getCloudBaseUrl(),
        ticket: trimmed,
      }),
    });
    if (!response?.ok) {
      throw new Error(responseErrorMessage(response));
    }
    return parsedSuccessBody(response);
  };
}

module.exports = {
  createDesktopTicketAuthenticator,
};
