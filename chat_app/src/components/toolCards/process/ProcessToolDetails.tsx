import React from 'react';

import { RowsCard, TextBlockCard, renderCardHeader } from '../shared/primitives';
import { asArray, asBoolean, asNumber, asRecord, asString } from '../shared/value';

const PROCESS_TOOL_NAMES = new Set([
  'execute_command',
  'get_recent_logs',
  'process_list',
  'process_poll',
  'process_log',
  'process_wait',
  'process_write',
  'process_kill',
  'process',
]);

const LogListCard: React.FC<{ title: string; items: unknown[] }> = ({ title, items }) => {
  const logs = items
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);

  if (logs.length === 0) return null;

  return (
    <div className="tool-detail-card tool-detail-card--full">
      {renderCardHeader(title, `${logs.length} 条`)}
      <div className="tool-detail-list">
        {logs.map((log, index) => (
          <div key={`${title}-${index}`} className="tool-detail-item">
            <div className="tool-detail-item-meta">
              {[asString(log.type).trim(), asString(log.created_at ?? log.createdAt).trim()].filter(Boolean).join(' · ')}
            </div>
            <div className="tool-detail-item-body">
              {asString(log.content).trim()}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

const TerminalListCard: React.FC<{ title: string; items: unknown[] }> = ({ title, items }) => {
  const terminals = items
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);

  if (terminals.length === 0) return null;

  return (
    <div className="tool-detail-card tool-detail-card--full">
      {renderCardHeader(title, `${terminals.length} 个`)}
      <div className="tool-detail-list">
        {terminals.map((terminal, index) => (
          <div key={`${title}-${index}`} className="tool-detail-item">
            <div className="tool-detail-item-title">
              {asString(terminal.terminal_name ?? terminal.name).trim() || `terminal ${index + 1}`}
            </div>
            <div className="tool-detail-item-meta">
              {[
                asString(terminal.process_status ?? terminal.status).trim(),
                asString(terminal.cwd).trim(),
              ].filter(Boolean).join(' · ')}
            </div>
            <div className="tool-detail-item-body">
              {asString(terminal.output_preview ?? terminal.output_tail).trim()}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

interface ProcessToolDetailsProps {
  displayName: string;
  result: unknown;
}

export const isProcessToolName = (displayName: string): boolean => PROCESS_TOOL_NAMES.has(displayName);

export const ProcessToolDetails: React.FC<ProcessToolDetailsProps> = ({
  displayName,
  result,
}) => {
  const record = asRecord(result);
  if (!record) return null;

  const action = asString(record.action).trim();
  const effectiveDisplayName = displayName === 'process' && action
    ? `process_${action}`
    : displayName;

  const terminals = asArray(record.terminals ?? record.processes);
  const logs = asArray(record.logs);
  const output = asString(record.output).trim();
  const outputPreview = asString(record.output_preview ?? record.outputPreview ?? record.output_tail).trim();

  if (effectiveDisplayName === 'execute_command') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Command status"
          rows={[
            { key: 'path', value: asString(record.path).trim() },
            { key: 'background', value: asBoolean(record.background) },
            { key: 'busy', value: asBoolean(record.busy) },
            { key: 'reused terminal', value: asBoolean(record.terminal_reused ?? record.terminalReused) },
            { key: 'finished by', value: asString(record.finished_by ?? record.finishedBy).trim() },
            { key: 'truncated', value: asBoolean(record.truncated) },
          ]}
        />
        <TextBlockCard title="Output" content={output || outputPreview} />
      </div>
    );
  }

  if (effectiveDisplayName === 'get_recent_logs') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Terminal summary"
          rows={[
            { key: 'scope', value: asString(record.result_scope ?? record.resultScope).trim() },
            { key: 'terminals', value: asNumber(record.terminal_count ?? record.terminalCount) },
          ]}
        />
        <TerminalListCard title="Recent terminals" items={terminals} />
      </div>
    );
  }

  if (effectiveDisplayName === 'process_list' || effectiveDisplayName === 'process_poll') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title={effectiveDisplayName === 'process_list' ? 'Process summary' : 'Process state'}
          rows={[
            { key: 'status', value: asString(record.process_status ?? record.status).trim() },
            { key: 'busy', value: asBoolean(record.busy) },
            { key: 'returned logs', value: asNumber(record.returned_log_count ?? record.returnedLogCount) },
            { key: 'has more', value: asBoolean(record.has_more ?? record.hasMore) },
            { key: 'truncated', value: asBoolean(record.truncated) },
          ]}
        />
        <TerminalListCard title="Processes" items={terminals} />
        <LogListCard title="Recent logs" items={logs} />
      </div>
    );
  }

  if (effectiveDisplayName === 'process_log') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Log window"
          rows={[
            { key: 'showing', value: asString(record.showing).trim() },
            { key: 'total lines', value: asNumber(record.total_lines ?? record.totalLines) },
            { key: 'has more', value: asBoolean(record.has_more ?? record.hasMore) },
          ]}
        />
        <TextBlockCard title="Process log" content={output} />
      </div>
    );
  }

  if (effectiveDisplayName === 'process_wait') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Wait result"
          rows={[
            { key: 'status', value: asString(record.wait_status ?? record.waitStatus).trim() },
            { key: 'completed', value: asBoolean(record.completed) },
            { key: 'timed out', value: asBoolean(record.timed_out ?? record.timedOut) },
            { key: 'waited ms', value: asNumber(record.waited_ms ?? record.waitedMs) },
            { key: 'exit code', value: asNumber(record.exit_code ?? record.exitCode) },
          ]}
        />
        <TextBlockCard title="Output" content={output || outputPreview} />
        <TextBlockCard title="Timeout note" content={asString(record.timeout_note ?? record.timeoutNote)} fullWidth={false} />
      </div>
    );
  }

  if (effectiveDisplayName === 'process_write' || effectiveDisplayName === 'process_submit' || effectiveDisplayName === 'process_close') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Input sent"
          rows={[
            { key: 'status', value: asString(record.operation_status ?? record.operationStatus).trim() },
            { key: 'submit', value: asBoolean(record.submit) },
            { key: 'written chars', value: asNumber(record.written_chars ?? record.writtenChars) },
            { key: 'busy', value: asBoolean(record.busy) },
          ]}
        />
      </div>
    );
  }

  if (effectiveDisplayName === 'process_kill') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Termination result"
          rows={[
            { key: 'status', value: asString(record.operation_status ?? record.operationStatus).trim() },
            { key: 'already exited', value: asBoolean(record.already_exited ?? record.alreadyExited) },
            { key: 'killed', value: asBoolean(record.killed) },
            { key: 'busy before', value: asBoolean(record.busy_before ?? record.busyBefore) },
            { key: 'busy after', value: asBoolean(record.busy_after ?? record.busyAfter) },
          ]}
        />
      </div>
    );
  }

  return (
    <div className="tool-detail-stack">
      <RowsCard
        title="Process details"
        rows={[
          { key: 'status', value: asString(record.status).trim() },
          { key: 'busy', value: asBoolean(record.busy) },
          { key: 'completed', value: asBoolean(record.completed) },
        ]}
      />
      <TerminalListCard title="Processes" items={terminals} />
      <LogListCard title="Recent logs" items={logs} />
      <TextBlockCard title="Output" content={output || outputPreview} />
    </div>
  );
};

export default ProcessToolDetails;
