// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Alert, Button } from 'antd';

import { ApiRequestError } from '../api/client';

interface QueryErrorAlertProps {
  error: unknown;
  loadFailedTitle: string;
  authorizationDescription: string;
  retryLabel: string;
  onRetry: () => void;
}

export function QueryErrorAlert({
  error,
  loadFailedTitle,
  authorizationDescription,
  retryLabel,
  onRetry,
}: QueryErrorAlertProps) {
  if (!error) {
    return null;
  }

  const authorizationFailed =
    error instanceof ApiRequestError && (error.status === 401 || error.status === 403);
  const description = authorizationFailed
    ? authorizationDescription
    : error instanceof Error
      ? error.message
      : String(error);

  return (
    <Alert
      type="error"
      showIcon
      title={loadFailedTitle}
      description={
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8, alignItems: 'flex-start' }}>
          <span>{description}</span>
          <Button size="small" onClick={onRetry}>
            {retryLabel}
          </Button>
        </div>
      }
    />
  );
}
