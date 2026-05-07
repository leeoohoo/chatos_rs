import type { MouseEvent } from 'react';
import { useEffect, useRef, useState } from 'react';

import { cn } from '../../../lib/utils';
import type { AgentConfig } from '../../../types';

interface InputAreaSkillPickerProps {
  currentAgent: AgentConfig | null;
  disabled: boolean;
  isStreaming: boolean;
  isStopping: boolean;
  skillsEnabled: boolean;
  onSkillsEnabledChange: (enabled: boolean) => void;
  skillsLoading: boolean;
  availableSkillOptions: Array<{ id: string; name: string; description?: string | null }>;
  selectedSkillIds: string[];
  onToggleSelectedSkill: (skillId: string) => void;
  onClearSelectedSkills: () => void;
}

export const InputAreaSkillPicker = ({
  currentAgent,
  disabled,
  isStreaming,
  isStopping,
  skillsEnabled,
  onSkillsEnabledChange,
  skillsLoading,
  availableSkillOptions,
  selectedSkillIds,
  onToggleSelectedSkill,
  onClearSelectedSkills,
}: InputAreaSkillPickerProps) => {
  const selectedSkillCount = selectedSkillIds.length;
  const skillsToggleDisabled = disabled || isStreaming || isStopping;
  const [skillPickerOpen, setSkillPickerOpen] = useState(false);
  const skillPickerRef = useRef<HTMLDivElement | null>(null);

  const handleSkillsButtonClick = (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
    if (skillsToggleDisabled) {
      return;
    }
    const nextEnabled = !skillsEnabled;
    onSkillsEnabledChange(nextEnabled);
    setSkillPickerOpen(nextEnabled);
  };

  const handleSkillPickerClick = (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
    if (disabled || isStreaming || isStopping) {
      return;
    }
    setSkillPickerOpen((prev) => !prev);
  };

  useEffect(() => {
    if (!skillsEnabled) {
      setSkillPickerOpen(false);
    }
  }, [skillsEnabled]);

  useEffect(() => {
    if (!skillPickerOpen) {
      return undefined;
    }
    const handleDocumentClick = (event: globalThis.MouseEvent) => {
      const target = event.target as Node;
      if (skillPickerRef.current && !skillPickerRef.current.contains(target)) {
        setSkillPickerOpen(false);
      }
    };
    document.addEventListener('mousedown', handleDocumentClick);
    return () => {
      document.removeEventListener('mousedown', handleDocumentClick);
    };
  }, [skillPickerOpen]);

  return (
    <div className="flex items-center gap-2">
      <button
        type="button"
        onClick={handleSkillsButtonClick}
        disabled={skillsToggleDisabled}
        className={cn(
          'flex-shrink-0 px-2 py-1 text-xs rounded-md transition-colors',
          skillsEnabled && !skillsToggleDisabled
            ? 'bg-emerald-600 text-white hover:bg-emerald-700'
            : 'bg-muted text-muted-foreground hover:text-foreground',
          skillsToggleDisabled && 'opacity-50 cursor-not-allowed',
        )}
        title={
          !currentAgent
            ? '启用技能上下文；发送时会按当前会话智能体解析'
            : (skillsEnabled ? '已启用技能上下文' : '未启用技能上下文')
        }
      >
        技能 {skillsEnabled && !skillsToggleDisabled ? '开' : '关'}
      </button>

      {skillsEnabled && (
        <div className="relative" ref={skillPickerRef}>
          <button
            type="button"
            onClick={handleSkillPickerClick}
            disabled={disabled || isStreaming || isStopping}
            className={cn(
              'flex-shrink-0 px-2 py-1 text-xs rounded-md border transition-colors',
              selectedSkillCount > 0
                ? 'border-emerald-600 bg-emerald-50 text-emerald-700'
                : 'border-border bg-background text-muted-foreground hover:text-foreground',
              (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed',
            )}
            title="选择本轮要直接注入全文的技能；不选则使用技能优先概览模式"
          >
            {selectedSkillCount > 0 ? `已选 ${selectedSkillCount}` : '技能选择'}
          </button>

          {skillPickerOpen && (
            <div className="absolute bottom-full left-0 z-50 mb-2 w-72 max-h-80 overflow-y-auto rounded-lg border border-border bg-popover p-2 shadow-lg">
              <div className="px-2 pb-2 text-xs text-muted-foreground">
                选择具体技能会把技能全文直接带入上下文；不选择则使用技能优先概览模式。
              </div>
              {availableSkillOptions.length > 0 ? (
                <div className="space-y-1">
                  {availableSkillOptions.map((skill) => {
                    const selected = selectedSkillIds.includes(skill.id);
                    return (
                      <button
                        key={skill.id}
                        type="button"
                        onClick={(event) => {
                          event.preventDefault();
                          onToggleSelectedSkill(skill.id);
                        }}
                        disabled={disabled || isStreaming || isStopping}
                        className={cn(
                          'w-full rounded-md border px-2 py-1.5 text-left text-xs transition-colors',
                          selected
                            ? 'border-emerald-600 bg-emerald-50 text-emerald-700'
                            : 'border-transparent hover:border-border hover:bg-accent',
                          (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed',
                        )}
                        title={skill.description || skill.name}
                      >
                        <span className="flex items-center gap-2">
                          <span
                            className={cn(
                              'flex h-3.5 w-3.5 items-center justify-center rounded-sm border text-[10px]',
                              selected
                                ? 'border-emerald-600 bg-emerald-600 text-white'
                                : 'border-border bg-background',
                            )}
                          >
                            {selected ? 'x' : ''}
                          </span>
                          <span className="min-w-0 flex-1 truncate">{skill.name}</span>
                        </span>
                        {skill.description ? (
                          <span className="mt-0.5 block truncate pl-5 text-[11px] text-muted-foreground">
                            {skill.description}
                          </span>
                        ) : null}
                      </button>
                    );
                  })}
                  {selectedSkillCount > 0 && (
                    <button
                      type="button"
                      onClick={(event) => {
                        event.preventDefault();
                        onClearSelectedSkills();
                      }}
                      disabled={disabled || isStreaming || isStopping}
                      className="mt-2 w-full rounded-md border border-dashed border-border px-2 py-1.5 text-xs text-muted-foreground hover:text-foreground"
                      title="清空已选技能，保留技能优先模式"
                    >
                      清空已选技能
                    </button>
                  )}
                </div>
              ) : (
                <div className="rounded-md border border-dashed border-border px-2 py-3 text-xs text-muted-foreground">
                  {skillsLoading
                    ? '正在加载当前智能体的技能...'
                    : '当前智能体暂无可选择技能。不选择具体技能时，发送会使用技能优先概览模式。'}
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
