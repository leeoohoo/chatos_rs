// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export type ApiRequestFn = <T>(endpoint: string, options?: RequestInit) => Promise<T>;

export interface SessionPaging {
  limit?: number;
  offset?: number;
  includeArchived?: boolean;
  includeArchiving?: boolean;
}

export interface ContactPaging {
  limit?: number;
  offset?: number;
}

export type { RemoteConnectionDraftPayload as RemoteConnectionPayload } from '../types';

export const buildRemoteVerificationHeaders = (
  verificationCode?: string,
): Record<string, string> | undefined => {
  const trimmed = verificationCode?.trim();
  return trimmed ? { 'x-remote-verification-code': trimmed } : undefined;
};
