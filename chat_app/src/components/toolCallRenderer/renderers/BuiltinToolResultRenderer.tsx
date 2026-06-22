import { BuiltinToolDetails, isBuiltinToolRenderable } from '../../BuiltinToolDetails';
import { getToolRendererSourceLabel } from '../../../i18n/toolText';
import type { ToolResultRenderer } from './types';

export const builtinToolResultRenderer: ToolResultRenderer = {
  id: 'builtin',
  sourceLabel: (locale) => getToolRendererSourceLabel('builtin', locale),
  matches: ({ toolName, parsedResult }) => isBuiltinToolRenderable(toolName, parsedResult),
  render: ({ toolName, parsedResult }) => (
    <BuiltinToolDetails rawToolName={toolName} result={parsedResult} />
  ),
};
