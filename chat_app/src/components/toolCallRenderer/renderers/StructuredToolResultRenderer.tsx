import GenericStructuredResultDetails from '../../toolCards/shared/GenericStructuredResultDetails';
import type { ToolResultRenderer } from './types';

export const structuredToolResultRenderer: ToolResultRenderer = {
  id: 'structured',
  sourceLabel: '通用面板',
  matches: ({ hasStructuredResult }) => hasStructuredResult,
  render: ({ structuredDisplayResult, structuredResultNote }) => (
    <>
      {structuredResultNote && (
        <div className="tool-structured-note">{structuredResultNote}</div>
      )}
      <GenericStructuredResultDetails value={structuredDisplayResult} />
    </>
  ),
};
