// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export function splitArgs(value: string): string[] {
  return value
    .split(/\s+/)
    .map((item) => item.trim())
    .filter(Boolean);
}

export function sourceLabel(source: string) {
  const labels: Record<string, string> = {
    chatos_terminal_exec: 'Chat OS 终端',
    chatos_terminal_session: 'Chat OS 终端',
    local_mcp: 'Task Runner',
    task_runner_sandbox: 'Task Runner',
    local_connector_ui: 'Local Connector 页面',
  };
  return labels[source] || source;
}

export function sourceGroup(source: string) {
  if (source === 'chatos_terminal_exec' || source === 'chatos_terminal_session') {
    return 'chatos_terminal';
  }
  if (source === 'local_mcp' || source === 'task_runner_sandbox') {
    return 'task_runner';
  }
  return source;
}

export function statusLabel(status: string) {
  const labels: Record<string, string> = {
    succeeded: '成功',
    failed: '失败',
    timed_out: '超时',
    submitted: '已提交',
    blocked: '已拦截',
  };
  return labels[status] || status;
}

export function historyStatusClass(status: string) {
  if (status === 'succeeded' || status === 'submitted') {
    return 'status ok';
  }
  if (status === 'failed' || status === 'timed_out' || status === 'blocked') {
    return 'status bad';
  }
  return 'status warn';
}

export function formatHistoryTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString();
}

export function formatTerminalResult(result: {
  stdout: string;
  stderr: string;
  exit_code?: number | null;
  success: boolean;
}) {
  return [
    `exit_code: ${result.exit_code ?? '-'}`,
    `success: ${result.success}`,
    '',
    result.stdout ? `stdout:\n${result.stdout}` : 'stdout: <empty>',
    result.stderr ? `stderr:\n${result.stderr}` : 'stderr: <empty>',
  ].join('\n');
}
