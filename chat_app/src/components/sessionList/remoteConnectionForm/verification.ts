// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { UI_MESSAGES } from '../../../i18n/messages';
import type { RemoteConnectionTestResult } from './types';

interface SecondFactorErrorPayload {
  challenge_prompt?: string;
  challengePrompt?: string;
}

interface SecondFactorErrorLike {
  code?: string;
  payload?: SecondFactorErrorPayload | null;
}

export const readRemoteHostName = (result: RemoteConnectionTestResult): string => {
  const rawHost = result.remote_host ?? result.remoteHost;
  return typeof rawHost === 'string' && rawHost.trim() ? ` (${rawHost.trim()})` : '';
};

export const isSecondFactorRequired = (error: unknown): boolean => {
  const candidate = error as SecondFactorErrorLike | null;
  return typeof candidate?.code === 'string' && candidate.code === 'second_factor_required';
};

export const extractSecondFactorPrompt = (
  error: unknown,
  fallback = UI_MESSAGES['zh-CN']['remoteConnection.verificationPrompt'] || 'remoteConnection.verificationPrompt',
): string => {
  const candidate = error as SecondFactorErrorLike | null;
  const prompt = candidate?.payload?.challenge_prompt ?? candidate?.payload?.challengePrompt;
  if (typeof prompt === 'string' && prompt.trim()) {
    return prompt.trim();
  }
  return fallback;
};
