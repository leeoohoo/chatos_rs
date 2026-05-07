import type { TerminalLogResponse } from '../../../lib/api/client/types';

export const extractFailureReasonFromLogs = (
  logs: TerminalLogResponse[],
  command: string,
): string | null => {
  const lines = logs
    .map((item) => String(item?.content || ''))
    .join('\n')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
  if (!lines.length) {
    return null;
  }

  const checks: RegExp[] = [
    /command not found/i,
    /no such file or directory/i,
    /permission denied/i,
    /traceback \(most recent call last\)/i,
    /\berr(or)?\b/i,
    /\bpanic\b/i,
    /\bexception\b/i,
    /\bfailed\b/i,
  ];
  for (let i = lines.length - 1; i >= 0; i -= 1) {
    const line = lines[i];
    if (checks.some((regex) => regex.test(line))) {
      return line;
    }
  }

  const cmd = command.toLowerCase();
  const likelyLongRunning = /(run|start|dev|serve|bootrun|spring-boot:run)/i.test(cmd)
    && !/(test|build|lint)/i.test(cmd);
  if (likelyLongRunning) {
    return '命令已退出，未检测到持续运行进程';
  }
  return null;
};
