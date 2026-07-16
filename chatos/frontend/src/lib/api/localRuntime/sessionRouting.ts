// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export const isLocalRuntimeSessionId = (sessionId?: string | null): boolean => (
  String(sessionId || '').trim().startsWith('lc_session_')
);

export const assertCloudSessionOperation = (
  sessionId: string | null | undefined,
  operation: string,
): void => {
  if (!isLocalRuntimeSessionId(sessionId)) {
    return;
  }
  throw new Error(`本地会话暂不支持“${operation}”；已阻止该请求发送到云端。`);
};
