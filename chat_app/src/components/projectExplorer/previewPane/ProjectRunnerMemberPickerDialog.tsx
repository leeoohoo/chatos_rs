import React from 'react';

import type { ProjectRunnerMember } from '../useProjectExplorerRunState';

interface ProjectRunnerMemberPickerDialogProps {
  projectMembers: ProjectRunnerMember[];
  selectedMemberId: string | null;
  generationError: string | null;
  generating: boolean;
  runnerScriptPath: string;
  onSelectMember: (memberId: string) => void;
  onClose: () => void;
  onConfirm: () => void;
}

export const ProjectRunnerMemberPickerDialog: React.FC<ProjectRunnerMemberPickerDialogProps> = ({
  projectMembers,
  selectedMemberId,
  generationError,
  generating,
  runnerScriptPath,
  onSelectMember,
  onClose,
  onConfirm,
}) => (
  <div className="fixed inset-0 z-50 flex items-center justify-center">
    <button
      type="button"
      className="absolute inset-0 bg-black/50"
      onClick={onClose}
      aria-label="关闭成员选择"
    />
    <div className="relative w-[520px] max-w-[calc(100vw-24px)] rounded-lg border border-border bg-card p-5 shadow-xl">
      <div className="mb-1 text-base font-semibold text-foreground">选择执行成员</div>
      <div className="mb-3 text-xs text-muted-foreground">
        请选择一个团队成员来生成 `{runnerScriptPath}`。
      </div>
      <div className="max-h-72 overflow-y-auto rounded border border-border">
        {projectMembers.map((member) => {
          const active = member.contactId === selectedMemberId;
          return (
            <button
              key={member.contactId}
              type="button"
              onClick={() => onSelectMember(member.contactId)}
              className={`w-full border-b border-border px-3 py-2 text-left last:border-b-0 ${active ? 'bg-accent' : 'hover:bg-accent/50'}`}
            >
              <div className="truncate text-sm text-foreground">{member.name || member.contactId}</div>
              <div className="truncate text-[11px] text-muted-foreground">{member.agentId}</div>
            </button>
          );
        })}
      </div>
      {generationError && (
        <div className="mt-3 text-xs text-destructive">{generationError}</div>
      )}
      <div className="mt-4 flex justify-end gap-2">
        <button
          type="button"
          onClick={onClose}
          disabled={generating}
          className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
        >
          取消
        </button>
        <button
          type="button"
          onClick={onConfirm}
          disabled={!selectedMemberId || generating}
          className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
        >
          {generating ? '提交中...' : '确认并执行'}
        </button>
      </div>
    </div>
  </div>
);
