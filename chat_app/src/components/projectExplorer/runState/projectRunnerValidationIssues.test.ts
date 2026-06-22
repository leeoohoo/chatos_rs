import { describe, expect, it } from 'vitest';

import { formatProjectRunValidationIssues } from './projectRunnerValidationIssues';

describe('projectRunnerValidationIssues', () => {
  it('formats validation issues into a single actionable message', () => {
    expect(formatProjectRunValidationIssues([
      { kind: 'toolchain', message: '缺少 JDK', targetLabel: 'Java App', hint: '请选择 JDK 21' },
    ] as never, 'fallback')).toBe('启动前检查未通过：[Java App] 缺少 JDK；建议：请选择 JDK 21');
  });
});
