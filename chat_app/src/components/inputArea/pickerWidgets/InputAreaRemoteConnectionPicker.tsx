import React from 'react';

import { cn } from '../../../lib/utils';
import type { RemoteConnection } from '../../../types';

interface InputAreaRemoteConnectionPickerProps {
  availableRemoteConnections: RemoteConnection[];
  currentRemoteConnectionId?: string | null;
  onRemoteConnectionChange?: (connectionId: string | null) => void;
  disabled: boolean;
  isStreaming: boolean;
  isStopping: boolean;
}

export const InputAreaRemoteConnectionPicker: React.FC<InputAreaRemoteConnectionPickerProps> = ({
  availableRemoteConnections,
  currentRemoteConnectionId,
  onRemoteConnectionChange,
  disabled,
  isStreaming,
  isStopping,
}) => {
  if (!Array.isArray(availableRemoteConnections) || availableRemoteConnections.length === 0) {
    return null;
  }

  return (
    <select
      value={currentRemoteConnectionId || ''}
      onChange={(event) => {
        const connectionId = event.target.value || null;
        onRemoteConnectionChange?.(connectionId);
      }}
      disabled={disabled || isStreaming || isStopping}
      className={cn(
        'flex-shrink-0 px-2 py-1 text-xs rounded-md border bg-background',
        'text-foreground focus:outline-none focus:ring-1 focus:ring-primary max-w-[220px]',
        (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed',
      )}
      title="选择远程服务器（会透传给 AI 工具）"
    >
      <option value="">
        服务器: 不选择
      </option>
      {availableRemoteConnections.map((connection) => (
        <option key={connection.id} value={connection.id}>
          {`服务器: ${connection.name || connection.host}`}
        </option>
      ))}
    </select>
  );
};
