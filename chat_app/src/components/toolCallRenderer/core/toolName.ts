export const CODE_READ_TOOL_NAMES = new Set([
  'read_file_raw',
  'read_file_range',
  'read_file',
]);

export const toolNameMatches = (actual: string, expected: string): boolean => (
  actual === expected || actual.endsWith(`_${expected}`)
);

export const isResearchToolName = (toolName: string): boolean => (
  toolNameMatches(toolName, 'browser_research')
  || toolNameMatches(toolName, 'web_research')
);
