import React from 'react';

import ChangeOperationDetails from './ChangeOperationDetails';
import ListDirDetails from './ListDirDetails';
import ReadFileDetails from './ReadFileDetails';
import SearchMatchesDetails from './SearchMatchesDetails';

const CHANGE_TOOLS = new Set([
  'write_file',
  'edit_file',
  'append_file',
  'delete_path',
  'apply_patch',
  'patch',
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

  if (CHANGE_TOOLS.has(displayName)) {
    return (
      <div className="tool-detail-stack">
        <ChangeOperationDetails displayName={displayName} result={result} />
      </div>
    );
  }

  return null;
};

export default CodeMaintainerToolDetails;
