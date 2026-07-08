// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  formatAssistantFailureContent,
  resolveReadableErrorMessage,
} from './errorParsing';

export const buildSendMessageFailure = (
  error: unknown,
  streamedText: string,
) => {
  const readableError = resolveReadableErrorMessage(error);
  return {
    failureContent: formatAssistantFailureContent(readableError, streamedText),
    readableError,
  };
};
