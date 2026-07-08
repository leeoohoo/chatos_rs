// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import GenericStructuredResultDetails from '../../toolCards/shared/GenericStructuredResultDetails';
import { getToolRendererSourceLabel } from '../../../i18n/toolText';
import type { ToolResultRenderer } from './types';

export const structuredToolResultRenderer: ToolResultRenderer = {
  id: 'structured',
  sourceLabel: (locale) => getToolRendererSourceLabel('structured', locale),
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
