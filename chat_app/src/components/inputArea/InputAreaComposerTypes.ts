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
  AgentConfig,
  FsEntry,
  Project,
  RemoteConnection,
} from '../../types';
import type { McpToolsetPreset, SelectableMcpConfig } from './useMcpSelection';

export interface InputAreaComposerProps {
  disabled: boolean;
  isStreaming: boolean;
  isStopping: boolean;
  isGuidingMode: boolean;
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
  currentAgent: AgentConfig | null;
  availableRemoteConnections: RemoteConnection[];
  onRemoteConnectionChange?: (connectionId: string | null) => void;
  taskRunnerAsyncContactMode: boolean;
  mcpEnabled: boolean;
  autoCreateTask: boolean;
  onMcpEnabledChange?: (enabled: boolean) => void;
  onAutoCreateTaskChange?: (enabled: boolean) => void;
  reasoningSupported: boolean;
  reasoningEnabled: boolean;
  onReasoningToggle?: (enabled: boolean) => void;
  placeholder: string;
  maxLength: number;
  supportedFileTypes: string[];
  isDragging: boolean;
  pickerRef: RefObject<HTMLDivElement>;
  mcpPickerRef: RefObject<HTMLDivElement>;
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
  mcpPickerOpen: boolean;
  handleToggleMcpPicker: () => void | Promise<void>;
  isAllMcpSelected: boolean;
  selectableMcpIds: string[];
  selectedMcpCount: number;
  mcpConfigsLoading: boolean;
  mcpConfigsError: string | null;
  availableMcpConfigs: SelectableMcpConfig[];
  builtinMcpConfigs: SelectableMcpConfig[];
  customMcpConfigs: SelectableMcpConfig[];
  mcpToolsetPresets: McpToolsetPreset[];
  projectScopeKey: string | null;
  hasProjectMcpDefault: boolean;
  hasDirectoryContext: boolean;
  hasRemoteContext: boolean;
  isProjectRequiredMcpId: (mcpId: string) => boolean;
  isRemoteRequiredMcpId: (mcpId: string) => boolean;
  sanitizedEnabledMcpIds: string[];
  loadAvailableMcpConfigs: (options?: { forceRefresh?: boolean }) => void | Promise<void>;
  handleSelectAllMcp: () => void;
  handleToggleMcpSelection: (mcpId: string) => void;
  handleApplyMcpToolsetPreset: (presetId: string) => void;
  handleSaveProjectMcpDefault: () => void;
  handleApplyProjectMcpDefault: () => void;
  skillsEnabled: boolean;
  onSkillsEnabledChange: (enabled: boolean) => void;
  skillsLoading: boolean;
  availableSkillOptions: Array<{ id: string; name: string; description?: string | null }>;
  selectedSkillIds: string[];
  onToggleSelectedSkill: (skillId: string) => void;
  onClearSelectedSkills: () => void;
  handleInputChange: (event: ChangeEvent<HTMLTextAreaElement>) => void;
  handleKeyDown: (event: KeyboardEvent<HTMLTextAreaElement>) => void;
  handlePaste: (event: ClipboardEvent<HTMLTextAreaElement>) => void;
  onStop?: () => void;
  handleSend: () => void;
  canSend: boolean;
  handleDragOver: (event: DragEvent<HTMLDivElement>) => void;
  handleDragLeave: (event: DragEvent<HTMLDivElement>) => void;
  handleDrop: (event: DragEvent<HTMLDivElement>) => void;
  handleFileSelect: (event: ChangeEvent<HTMLInputElement>) => void;
}
