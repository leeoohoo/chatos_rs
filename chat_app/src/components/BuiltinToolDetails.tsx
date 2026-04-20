import React from 'react';

import { resolveToolFamily } from '../lib/tools/catalog';
import { getToolDisplayName } from '../lib/tools/displayName';
import AgentBuilderToolDetails from './toolCards/agentBuilder/AgentBuilderToolDetails';
import BrowserToolDetails from './toolCards/browser/BrowserToolDetails';
import CodeMaintainerToolDetails from './toolCards/codeMaintainer/CodeMaintainerToolDetails';
import MemoryToolDetails from './toolCards/memory/MemoryToolDetails';
import NotepadToolDetails from './toolCards/notepad/NotepadToolDetails';
import ProcessToolDetails, { isProcessToolName } from './toolCards/process/ProcessToolDetails';
import RemoteToolDetails from './toolCards/remote/RemoteToolDetails';
import TaskManagerToolDetails from './toolCards/taskManager/TaskManagerToolDetails';
import UiPrompterToolDetails from './toolCards/uiPrompter/UiPrompterToolDetails';
import WebToolDetails from './toolCards/web/WebToolDetails';
import { asRecord } from './toolCards/shared/value';

const CODE_MAINTAINER_TOOLS = new Set([
  'read_file_raw',
  'read_file_range',
  'read_file',
  'list_dir',
  'search_text',
  'search_files',
  'write_file',
  'edit_file',
  'append_file',
  'delete_path',
  'apply_patch',
  'patch',
]);

const isBrowserToolName = (name: string): boolean => name.startsWith('browser_');
const isWebToolName = (name: string): boolean => name.startsWith('web_');
const isCodeMaintainerToolName = (name: string): boolean => CODE_MAINTAINER_TOOLS.has(name);

export const isBuiltinToolRenderable = (rawToolName: string, result: unknown): boolean => {
  if (!asRecord(result)) {
    return false;
  }

  const displayName = getToolDisplayName(rawToolName);
  return resolveToolFamily(rawToolName, displayName) !== 'generic';
};

interface BuiltinToolDetailsProps {
  rawToolName: string;
  result: unknown;
}

export const BuiltinToolDetails: React.FC<BuiltinToolDetailsProps> = ({
  rawToolName,
  result,
}) => {
  if (!asRecord(result)) {
    return null;
  }

  const displayName = getToolDisplayName(rawToolName);
  const family = resolveToolFamily(rawToolName, displayName);

  if (family === 'code' && isCodeMaintainerToolName(displayName)) {
    return <CodeMaintainerToolDetails displayName={displayName} result={result} />;
  }

  if (family === 'browser' && isBrowserToolName(displayName)) {
    return <BrowserToolDetails displayName={displayName} result={result} />;
  }

  if (family === 'web' && isWebToolName(displayName)) {
    return <WebToolDetails displayName={displayName} result={result} />;
  }

  if (family === 'process' && isProcessToolName(displayName)) {
    return <ProcessToolDetails displayName={displayName} result={result} />;
  }

  if (family === 'remote') {
    return <RemoteToolDetails displayName={displayName} result={result} />;
  }

  if (family === 'notepad') {
    return <NotepadToolDetails displayName={displayName} result={result} />;
  }

  if (family === 'task') {
    return <TaskManagerToolDetails displayName={displayName} result={result} />;
  }

  if (family === 'ui') {
    return <UiPrompterToolDetails displayName={displayName} result={result} />;
  }

  if (family === 'agent') {
    return <AgentBuilderToolDetails displayName={displayName} result={result} />;
  }

  if (family === 'memory') {
    return <MemoryToolDetails displayName={displayName} result={result} />;
  }

  return null;
};

export default BuiltinToolDetails;
