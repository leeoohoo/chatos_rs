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
