import type { MouseEvent } from 'react';
import { useEffect, useRef, useState } from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
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
  const { t } = useI18n();
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
            ? t('inputArea.skill.enableHint')
            : (skillsEnabled ? t('inputArea.skill.enabledHint') : t('inputArea.skill.disabledHint'))
        }
      >
        {t('inputArea.skill.button', { state: skillsEnabled && !skillsToggleDisabled ? t('composer.toggle.on') : t('composer.toggle.off') })}
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
            title={t('inputArea.skill.selectTitle')}
          >
            {selectedSkillCount > 0 ? t('inputArea.skill.selectedCount', { count: selectedSkillCount }) : t('inputArea.skill.selectButton')}
          </button>

          {skillPickerOpen && (
            <div className="absolute bottom-full left-0 z-50 mb-2 w-72 max-h-80 overflow-y-auto rounded-lg border border-border bg-popover p-2 shadow-lg">
              <div className="px-2 pb-2 text-xs text-muted-foreground">
                {t('inputArea.skill.description')}
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
                      title={t('inputArea.skill.clearTitle')}
                    >
                      {t('inputArea.skill.clear')}
                    </button>
                  )}
                </div>
              ) : (
                <div className="rounded-md border border-dashed border-border px-2 py-3 text-xs text-muted-foreground">
                  {skillsLoading
                    ? t('inputArea.skill.loading')
                    : t('inputArea.skill.empty')}
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
