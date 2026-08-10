// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import ChangeOperationDetails from './ChangeOperationDetails';
import ListDirDetails from './ListDirDetails';
import ReadFileDetails from './ReadFileDetails';
import SearchMatchesDetails from './SearchMatchesDetails';

const EDIT_SESSION_TOOLS = new Set([
  'open_edit_session',
  'stage_edit_batch',
  'commit_edit_session',
  'abort_edit_session',
]);

interface CodeMaintainerToolDetailsProps {
  displayName: string;
  result: unknown;
}

export const CodeMaintainerToolDetails: React.FC<CodeMaintainerToolDetailsProps> = ({
  displayName,
  result,
}) => {
  if (displayName === 'read_file_raw' || displayName === 'read_file_range' || displayName === 'read_file') {
    return (
      <div className="tool-detail-stack">
        <ReadFileDetails result={result} />
      </div>
    );
  }

  if (displayName === 'list_dir') {
    return (
      <div className="tool-detail-stack">
        <ListDirDetails result={result} />
      </div>
    );
  }

  if (displayName === 'search_text' || displayName === 'search_files') {
    return (
      <div className="tool-detail-stack">
        <SearchMatchesDetails result={result} />
      </div>
    );
  }

  if (EDIT_SESSION_TOOLS.has(displayName)) {
    return (
      <div className="tool-detail-stack">
        <ChangeOperationDetails result={result} />
      </div>
    );
  }

  return null;
};

export default CodeMaintainerToolDetails;
