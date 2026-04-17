import React from 'react';

import GenericStructuredResultDetails from '../shared/GenericStructuredResultDetails';
import { RowsCard, TextBlockCard, renderCardHeader } from '../shared/primitives';
import { asArray, asBoolean, asNumber, asRecord, asString } from '../shared/value';

const ConnectionListCard: React.FC<{ items: unknown[] }> = ({ items }) => {
  const connections = items
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);

  if (connections.length === 0) return null;

  return (
    <div className="tool-detail-card tool-detail-card--full">
      {renderCardHeader('Connections', `${connections.length} 个`)}
      <div className="tool-detail-list">
        {connections.map((connection, index) => {
          const title = asString(connection.name).trim() || `connection ${index + 1}`;
          const host = asString(connection.host).trim();
          const port = asNumber(connection.port);
          const username = asString(connection.username).trim();
          const authType = asString(connection.auth_type ?? connection.authType).trim();
          const defaultPath = asString(
            connection.default_remote_path ?? connection.defaultRemotePath,
          ).trim();

          return (
            <div key={`remote-connection-${index}`} className="tool-detail-item">
              <div className="tool-detail-item-title">{title}</div>
              <div className="tool-detail-item-meta">
                {[
                  username && host ? `${username}@${host}${port !== null ? `:${port}` : ''}` : host,
                  authType,
                ].filter(Boolean).join(' · ')}
              </div>
              {defaultPath && <div className="tool-detail-item-body">{defaultPath}</div>}
            </div>
          );
        })}
      </div>
    </div>
  );
};

const RemoteEntriesCard: React.FC<{ items: unknown[] }> = ({ items }) => {
  const entries = items
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);

  if (entries.length === 0) return null;

  return (
    <div className="tool-detail-card tool-detail-card--full">
      {renderCardHeader('Remote entries', `${entries.length} 项`)}
      <div className="tool-detail-list">
        {entries.map((entry, index) => (
          <div key={`remote-entry-${index}`} className="tool-detail-item">
            <div className="tool-detail-item-title">
              {asString(entry.name).trim() || `entry ${index + 1}`}
            </div>
            <div className="tool-detail-item-meta">
              {[
                asString(entry.path).trim(),
                asBoolean(entry.is_dir ?? entry.isDir) ? 'dir' : 'file',
                asNumber(entry.size) !== null ? `${asNumber(entry.size)} B` : '',
                asString(entry.modified_at ?? entry.modifiedAt).trim(),
              ].filter(Boolean).join(' · ')}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

interface RemoteToolDetailsProps {
  displayName: string;
  result: unknown;
}

export const RemoteToolDetails: React.FC<RemoteToolDetailsProps> = ({
  displayName,
  result,
}) => {
  const record = asRecord(result);
  if (!record) return null;

  if (displayName === 'list_connections') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Connection summary"
          rows={[
            { key: 'count', value: asNumber(record.count) ?? asArray(record.connections).length },
          ]}
        />
        <ConnectionListCard items={asArray(record.connections)} />
      </div>
    );
  }

  if (displayName === 'run_command') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Remote command"
          rows={[
            { key: 'connection', value: asString(record.name).trim() },
            { key: 'host', value: asString(record.host).trim() },
            { key: 'command', value: asString(record.command).trim() },
            { key: 'timeout seconds', value: asNumber(record.timeout_seconds ?? record.timeoutSeconds) },
            { key: 'output chars', value: asNumber(record.output_chars ?? record.outputChars) },
            { key: 'truncated', value: asBoolean(record.output_truncated ?? record.outputTruncated) },
          ]}
          fullWidth
        />
        <TextBlockCard title="Command output" content={asString(record.output)} />
      </div>
    );
  }

  if (displayName === 'list_directory') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Remote directory"
          rows={[
            { key: 'path', value: asString(record.path).trim() },
            { key: 'entries', value: asNumber(record.count) },
            { key: 'truncated', value: asBoolean(record.entries_truncated ?? record.entriesTruncated) },
          ]}
        />
        <RemoteEntriesCard items={asArray(record.entries)} />
      </div>
    );
  }

  if (displayName === 'read_file') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Remote file"
          rows={[
            { key: 'path', value: asString(record.path).trim() },
            { key: 'truncated', value: asBoolean(record.truncated) },
            { key: 'source size', value: asNumber(record.source_size_bytes ?? record.sourceSizeBytes) },
          ]}
        />
        <TextBlockCard title="Remote file content" content={asString(record.content)} />
      </div>
    );
  }

  if (displayName === 'test_connection') {
    const connectionResult = asRecord(record.result);

    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Connection target"
          rows={[
            { key: 'name', value: asString(record.name).trim() },
            { key: 'host', value: asString(record.host).trim() },
            { key: 'port', value: asNumber(record.port) },
            { key: 'username', value: asString(record.username).trim() },
          ]}
        />
        {connectionResult ? (
          <RowsCard
            title="Connection result"
            rows={[
              { key: 'success', value: asBoolean(connectionResult.success) },
              { key: 'remote host', value: asString(connectionResult.remote_host ?? connectionResult.remoteHost).trim() },
              { key: 'connected at', value: asString(connectionResult.connected_at ?? connectionResult.connectedAt).trim() },
            ]}
            fullWidth
          />
        ) : null}
        {!connectionResult && <GenericStructuredResultDetails value={record.result} />}
      </div>
    );
  }

  return (
    <div className="tool-detail-stack">
      <GenericStructuredResultDetails value={record} />
    </div>
  );
};

export default RemoteToolDetails;
