import { BuiltinToolDetails, isBuiltinToolRenderable } from '../../BuiltinToolDetails';
import type { ToolResultRenderer } from './types';

export const builtinToolResultRenderer: ToolResultRenderer = {
  id: 'builtin',
  sourceLabel: '内置面板',
  matches: ({ toolName, parsedResult }) => isBuiltinToolRenderable(toolName, parsedResult),
  render: ({ toolName, parsedResult }) => (
    <BuiltinToolDetails rawToolName={toolName} result={parsedResult} />
  ),
};
