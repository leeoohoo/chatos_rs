// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  ChangeEvent,
  ClipboardEvent,
  Dispatch,
  DragEvent,
  KeyboardEvent,
  RefObject,
  SetStateAction,
} from 'react';

import type {
  AiModelConfig,
  FsEntry,
  Project,
  RemoteConnection,
} from '../../types';

export interface InputAreaComposerProps {
  disabled: boolean;
  effectiveAllowAttachments: boolean;
  showModelSelector: boolean;
  selectedModelId: string | null;
  selectedModelName: string | null;
  selectedThinkingLevel: string | null;
  onModelChange?: (modelId: string | null) => void;
  onModelNameChange?: (modelName: string | null) => void;
  onThinkingLevelChange?: (level: string | null) => void;
  onModelRuntimeChange?: (selection: {
    selectedModelId?: string | null;
    selectedModelName?: string | null;
    selectedThinkingLevel?: string | null;
  }) => void;
  availableProjects: Project[];
  selectedProjectId: string | null;
  onProjectChange?: (projectId: string | null) => void;
  showProjectSelector: boolean;
  showWorkspaceRootPicker: boolean;
  currentRemoteConnectionId: string | null;
  availableRemoteConnections: RemoteConnection[];
  onRemoteConnectionChange?: (connectionId: string | null) => void;
  reasoningSupported: boolean;
  reasoningEnabled: boolean;
  onReasoningToggle?: (enabled: boolean) => void;
  planModeAvailable: boolean;
  planModeEnabled: boolean;
  onPlanModeToggle?: (enabled: boolean) => void;
  placeholder: string;
  maxLength: number;
  supportedFileTypes: string[];
  isDragging: boolean;
  pickerRef: RefObject<HTMLDivElement>;
  workspacePickerRef: RefObject<HTMLDivElement>;
  projectFilePickerRef: RefObject<HTMLDivElement>;
  fileInputRef: RefObject<HTMLInputElement>;
  textareaRef: RefObject<HTMLTextAreaElement>;
  message: string;
  setPickerOpen: Dispatch<SetStateAction<boolean>>;
  pickerOpen: boolean;
  hasAiOptions: boolean;
  currentAiLabel: string;
  effectiveModelName: string | null;
  effectiveThinkingLevel: string | null;
  enabledModels: AiModelConfig[];
  projectForFilePicker: Project | null;
  showProjectFilePicker: boolean;
  projectFileAttachingPath: string | null;
  projectFilePickerOpen: boolean;
  handleToggleProjectFilePicker: () => void | Promise<void>;
  projectFilePathLabel: string;
  projectFileFilter: string;
  setProjectFileFilter: (value: string) => void;
  projectFileBusy: boolean;
  projectFileKeywordActive: boolean;
  projectFileParent: string | null;
  loadProjectFileEntries: (path?: string | null) => void | Promise<void>;
  displayedProjectFileEntries: FsEntry[];
  handleAttachProjectFile: (entry: FsEntry) => void | Promise<void>;
  toRelativeProjectPath: (path: string) => string;
  projectFileSearchTruncated: boolean;
  normalizedWorkspaceRoot: string | null;
  workspaceRootDisplayName: string;
  workspacePickerOpen: boolean;
  workspacePath: string | null;
  workspaceParent: string | null;
  workspaceLoading: boolean;
  workspaceEntries: FsEntry[];
  workspaceRoots: FsEntry[];
  handleToggleWorkspacePicker: () => void | Promise<void>;
  loadWorkspaceDirectories: (path?: string | null) => void | Promise<void>;
  handleSelectWorkspaceRoot: (path: string | null) => void;
  handleInputChange: (event: ChangeEvent<HTMLTextAreaElement>) => void;
  handleKeyDown: (event: KeyboardEvent<HTMLTextAreaElement>) => void;
  handlePaste: (event: ClipboardEvent<HTMLTextAreaElement>) => void;
  handleSend: () => void;
  canSend: boolean;
  handleDragOver: (event: DragEvent<HTMLDivElement>) => void;
  handleDragLeave: (event: DragEvent<HTMLDivElement>) => void;
  handleDrop: (event: DragEvent<HTMLDivElement>) => void;
  handleFileSelect: (event: ChangeEvent<HTMLInputElement>) => void;
}
